use std::{collections::HashMap, io::Read};

use tracing::warn;
use xcursor::{
    parser::{parse_xcursor, Image},
    CursorTheme,
};

/// Nomes de cursor que o compositor escolhe na hora baseado em contexto
/// (borda da janela, header, conteúdo). Cliente também pode pedir um
/// desses via `wl_pointer.set_cursor` com Named, mas a maioria usa
/// Surface direto (cliente desenha o cursor próprio).
pub const NAMES: &[&str] = &[
    "default",
    "n-resize",
    "s-resize",
    "e-resize",
    "w-resize",
    "ne-resize",
    "nw-resize",
    "se-resize",
    "sw-resize",
];

pub struct Cursor {
    images: HashMap<&'static str, Vec<Image>>,
}

impl Cursor {
    pub fn load() -> Option<Self> {
        let theme_name = std::env::var("XCURSOR_THEME")
            .unwrap_or_else(|_| "default".to_string());
        let theme = CursorTheme::load(&theme_name);

        let mut images: HashMap<&'static str, Vec<Image>> = HashMap::new();
        for &name in NAMES {
            if let Some(loaded) = load_icon(&theme, name) {
                images.insert(name, loaded);
            } else if name == "default" {
                // Sem cursor default, compositor fica sem cursor visível.
                warn!("xcursor: tema sem cursor 'default'");
                return None;
            }
        }
        Some(Self { images })
    }

    /// Image mais próxima do tamanho desejado pro `name`, caindo no
    /// "default" se o nome não foi encontrado no tema.
    pub fn image_for(&self, name: &str) -> Option<&Image> {
        let desired_size: u32 = std::env::var("XCURSOR_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(24);

        self.images
            .get(name)
            .or_else(|| self.images.get("default"))
            .and_then(|images| {
                images
                    .iter()
                    .min_by_key(|img| (img.size as i32 - desired_size as i32).abs())
            })
    }
}

fn load_icon(theme: &CursorTheme, name: &str) -> Option<Vec<Image>> {
    let path = theme.load_icon(name)?;
    let mut file = std::fs::File::open(path).ok()?;
    let mut data = Vec::new();
    file.read_to_end(&mut data).ok()?;
    parse_xcursor(&data)
}
