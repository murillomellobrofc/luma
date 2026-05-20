use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            element::{
                memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
                AsRenderElements, Kind,
            },
            ImportAll, ImportMem, Renderer, Texture,
        },
    },
    desktop::space::SpaceElement,
    output::Output,
    utils::{IsAlive, Logical, Physical, Point, Rectangle, Scale, Size, Transform},
};
use tracing::{info, warn};

use crate::render::LumaRenderElement;

/// Wallpaper de fundo carregado uma vez no boot. `buffer` é Argb8888
/// premultiplied (igual cursor e texto), `size` é a dimensão original
/// da imagem em pixels — o caller passa pro RenderElement como size de
/// destino esticando pra ocupar o output.
pub struct Wallpaper {
    pub buffer: MemoryRenderBuffer,
    pub size: Size<i32, Logical>,
    /// Tamanho atual do output onde o wallpaper é renderizado. Atualizado
    /// pelo backend quando a janela winit redimensiona. Mutex pra
    /// permitir &SpaceElement (interior mutability).
    output_size: Mutex<Size<i32, Logical>>,
    /// Output ao qual este wallpaper está vinculado. Setado em
    /// `attach_to_output`. Hoje é só metadata (output_size continua sendo
    /// a fonte de verdade pro render); preparação pra multi-monitor onde
    /// cada output terá seu próprio Wallpaper. H34.2.
    output: Mutex<Option<Output>>,
}

impl Wallpaper {
    pub fn set_output_size(&self, size: Size<i32, Logical>) {
        *self.output_size.lock().unwrap() = size;
    }

    /// Vincula este wallpaper a um output específico. Clona o `Output`
    /// (Arc internamente). Nas próximas etapas, cada Wallpaper vai pertencer
    /// a 1 Output (multi-monitor). Hoje é só registro — o render continua
    /// usando `output_size` setado via `set_output_size`.
    pub fn attach_to_output(&self, output: &Output) {
        *self.output.lock().unwrap() = Some(output.clone());
    }

    /// Output atualmente vinculado, se houver. Útil pra subsystems futuros
    /// que precisem identificar o monitor (ex: posicionar overlays).
    #[allow(dead_code)] // Será consumido em H34.5/H34.6.
    pub fn output(&self) -> Option<Output> {
        self.output.lock().unwrap().clone()
    }

    fn current_output_size(&self) -> Size<i32, Logical> {
        *self.output_size.lock().unwrap()
    }

    /// Tenta carregar `~/.config/luma/wallpaper.png` ou `.jpg`. `None`
    /// se nenhum dos dois existir ou se decode falhar — caller usa
    /// clear color como fallback.
    pub fn load() -> Option<Self> {
        let candidates = config_paths();
        for path in &candidates {
            if !path.exists() {
                continue;
            }
            match try_load(path) {
                Some(w) => {
                    info!("wallpaper carregado de {:?} ({}x{})", path, w.size.w, w.size.h);
                    return Some(w);
                }
                None => warn!("falha ao decodificar {:?}", path),
            }
        }
        info!("nenhum wallpaper em {candidates:?}, usando clear color do tema");
        None
    }
}

fn config_paths() -> Vec<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    let Some(base) = base else {
        return Vec::new();
    };
    let dir = base.join("luma");
    vec![
        dir.join("wallpaper.png"),
        dir.join("wallpaper.jpg"),
        dir.join("wallpaper.jpeg"),
    ]
}

fn try_load(path: &Path) -> Option<Wallpaper> {
    let img = image::open(path).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width() as i32, rgba.height() as i32);
    let pixels = swizzle_rgba_to_bgra_premultiplied(rgba.into_raw());

    let buffer = MemoryRenderBuffer::from_slice(
        &pixels,
        Fourcc::Argb8888,
        (w, h),
        1,
        Transform::Normal,
        None,
    );
    Some(Wallpaper {
        buffer,
        size: Size::from((w, h)),
        output_size: Mutex::new(Size::from((w, h))),
        output: Mutex::new(None),
    })
}

impl IsAlive for Wallpaper {
    fn alive(&self) -> bool {
        true
    }
}

/// Wallpaper se comporta como SpaceElement com z_index = 0 (renderizado
/// atrás de tudo) ocupando o output inteiro. Não recebe input.
impl SpaceElement for Wallpaper {
    fn bbox(&self) -> Rectangle<i32, Logical> {
        Rectangle::from_size(self.current_output_size())
    }

    fn geometry(&self) -> Rectangle<i32, Logical> {
        Rectangle::from_size(self.current_output_size())
    }

    fn is_in_input_region(&self, _point: &Point<f64, Logical>) -> bool {
        false
    }

    fn set_activate(&self, _activated: bool) {}
    fn output_enter(&self, _output: &Output, _overlap: Rectangle<i32, Logical>) {}
    fn output_leave(&self, _output: &Output) {}
    fn refresh(&self) {}

    fn z_index(&self) -> u8 {
        0
    }
}

impl<R> AsRenderElements<R> for Wallpaper
where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Texture + Clone + Send + 'static,
{
    type RenderElement = LumaRenderElement<R>;

    fn render_elements<C: From<LumaRenderElement<R>>>(
        &self,
        renderer: &mut R,
        location: Point<i32, Physical>,
        _scale: Scale<f64>,
        _alpha: f32,
    ) -> Vec<C> {
        use crate::theme::WallpaperMode;
        let output = self.current_output_size();
        let image = self.size;
        let theme = crate::theme::current();

        let (dst_loc, dst_size) = match theme.wallpaper_mode {
            WallpaperMode::Stretch => (location, output),
            WallpaperMode::Center => {
                let off_x = (output.w - image.w) / 2;
                let off_y = (output.h - image.h) / 2;
                let loc = Point::<i32, Physical>::from((location.x + off_x, location.y + off_y));
                (loc, image)
            }
            WallpaperMode::Cover => {
                // Escala pra cobrir o output mantendo proporção; corta sobra.
                let sx = output.w as f64 / image.w as f64;
                let sy = output.h as f64 / image.h as f64;
                let s = sx.max(sy);
                let scaled_w = (image.w as f64 * s).round() as i32;
                let scaled_h = (image.h as f64 * s).round() as i32;
                let off_x = (output.w - scaled_w) / 2;
                let off_y = (output.h - scaled_h) / 2;
                let loc =
                    Point::<i32, Physical>::from((location.x + off_x, location.y + off_y));
                (loc, smithay::utils::Size::from((scaled_w, scaled_h)))
            }
        };

        match MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            dst_loc.to_f64(),
            &self.buffer,
            None,
            None,
            Some(dst_size),
            Kind::Unspecified,
        ) {
            Ok(elem) => vec![C::from(LumaRenderElement::Memory(elem))],
            Err(_) => Vec::new(),
        }
    }
}

/// Converte de RGBA (image crate) pra BGRA premultiplied (formato
/// esperado pelo Argb8888 em little-endian + GLES blending).
fn swizzle_rgba_to_bgra_premultiplied(mut pixels: Vec<u8>) -> Vec<u8> {
    for chunk in pixels.chunks_exact_mut(4) {
        let r = chunk[0];
        let g = chunk[1];
        let b = chunk[2];
        let a = chunk[3];
        // premultiplied: cada canal multiplicado pelo alpha. Pra alpha 255
        // (caso típico de wallpaper opaco), `c * 255 / 255 == c`, então
        // não muda nada. Pra alpha intermediário (PNG com transparência),
        // o blend fica correto contra o clear color por trás.
        let pr = ((r as u16 * a as u16) / 255) as u8;
        let pg = ((g as u16 * a as u16) / 255) as u8;
        let pb = ((b as u16 * a as u16) / 255) as u8;
        chunk[0] = pb;
        chunk[1] = pg;
        chunk[2] = pr;
        chunk[3] = a;
    }
    pixels
}
