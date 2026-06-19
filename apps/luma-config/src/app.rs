//! App iced do luma-config — padrão Elm (State / Message / update / view).
//!
//! **Frameless** (o Luma decora server-side). 2b.2: a landing é a
//! `CategoryGrid` renderizando a taxonomia (6 seções), com uma barra de
//! busca estática no topo. O dado da taxonomia mora aqui (é a estrutura
//! deste app); o `libluma` fornece só o componente. Navegação ao clicar
//! (2c), busca funcional (2e) e ícones de verdade (2b.3) ainda não entram.

use iced::widget::{column, container, responsive, scrollable, svg, text, text_input};
use iced::{Background, Border, Color, Element, Length, Padding, Task, Theme};
use libluma::{GoldenHour, GridItem, GridSection};

/// Estado do app: paleta + taxonomia + qual view está aberta + estado
/// local dos controles (sem efeito real ainda; o wiring no compositor via
/// D-Bus é um passo depois).
pub struct State {
    gh: GoldenHour,
    taxonomy: Vec<GridSection<'static, Message>>,
    view: View,
    /// Texto da busca da landing (2e). Vazio = grade inteira.
    query: String,
    /// Conexão D-Bus com o compositor (3a). Estabelecida no boot; `None`
    /// até conectar (ou se o compositor não estiver no bus). Clonável e
    /// barata (Arc por dentro) — cada `Task` clona pra fazer a chamada.
    conn: Option<zbus::Connection>,
    touch: TouchTargets,
    contrast: Contrast,
    motion: Motion,
    bell: VisualBell,
}

/// Tela visível: a grade (landing) ou a página de uma categoria.
#[derive(Clone, Copy)]
enum View {
    Grid,
    Category(&'static str),
}

/// Estado local da página **Alvos de toque** (2d). Modelo A: cada
/// categoria tem suas próprias configurações. Sem efeito real ainda.
#[derive(Debug, Clone, Copy)]
struct TouchTargets {
    /// Alvos de toque maiores (default ligado, igual o mockup).
    larger: bool,
    /// Tamanho mínimo do alvo em px (>= 44, o piso WCAG AAA).
    min_size: f32,
    /// Realçar o foco do teclado.
    highlight_focus: bool,
}

impl Default for TouchTargets {
    fn default() -> Self {
        Self {
            larger: true,
            min_size: 44.0,
            highlight_focus: false,
        }
    }
}

/// Estado local da página **Contraste** (2d.2).
#[derive(Debug, Clone, Copy, Default)]
struct Contrast {
    high_contrast: bool,
    reduce_transparency: bool,
}

/// Estado local da página **Movimento** (2d.2). `anim_speed` em %
/// (100 = normal).
#[derive(Debug, Clone, Copy)]
struct Motion {
    reduce_motion: bool,
    anim_speed: f32,
}

impl Default for Motion {
    fn default() -> Self {
        Self {
            reduce_motion: false,
            anim_speed: 100.0,
        }
    }
}

/// Estado local da página **Sino visual** (2d.2). `photosafe` (modo seguro
/// pra fotossensibilidade) vem **ligado** por padrão — flash suave, sem
/// estrobo. `flash_intensity` em %.
#[derive(Debug, Clone, Copy)]
struct VisualBell {
    enabled: bool,
    photosafe: bool,
    flash_intensity: f32,
}

impl Default for VisualBell {
    fn default() -> Self {
        Self {
            enabled: false,
            photosafe: true,
            flash_intensity: 60.0,
        }
    }
}

impl State {
    pub fn new() -> Self {
        let gh = GoldenHour::load();
        let taxonomy = build_taxonomy(&gh);
        Self {
            gh,
            taxonomy,
            view: View::Grid,
            query: String::new(),
            conn: None,
            touch: TouchTargets::default(),
            contrast: Contrast::default(),
            motion: Motion::default(),
            bell: VisualBell::default(),
        }
    }
}

/// Boot do app (3a): estado inicial + um `Task` que estabelece a conexão
/// D-Bus com o compositor em background. O resultado volta como
/// [`Message::Connected`] e fica guardado no estado. A UI sobe na hora; se
/// a conexão falhar, os controles ligados ao backend só não terão efeito.
pub fn boot() -> (State, Task<Message>) {
    (
        State::new(),
        Task::perform(crate::ipc::connect(), Message::Connected),
    )
}

/// Mensagens da UI (navegação 2c + controles locais 2d).
#[derive(Debug, Clone)]
pub enum Message {
    /// Abrir a página de uma categoria (o `&str` é o título/id dela).
    OpenCategory(&'static str),
    /// Voltar pra grade.
    Back,
    /// Texto da busca mudou (2e).
    SearchChanged(String),
    /// Resultado da conexão D-Bus do boot (3a). Ok guarda a conexão.
    Connected(Result<zbus::Connection, String>),
    /// Os 3 settings do Sino visual lidos do compositor ao abrir a página
    /// (3c): `(visual_bell, photosafe, intensidade%)`.
    BellSettingsLoaded(Result<(bool, bool, u8), String>),
    /// Resultado de escrever o sino visual (3a). Carrega o valor tentado
    /// pra reverter o otimismo se a escrita falhar.
    BellWritten(bool, Result<(), String>),
    /// Resultado de escrever o photosafe (3c); reverte se falhar.
    PhotosafeWritten(bool, Result<(), String>),
    /// Slider de intensidade SOLTO (3c) — é aqui que commita no D-Bus/disco.
    BellIntensityReleased,
    /// Resultado de escrever a intensidade (3c); só loga se falhar.
    IntensityWritten(Result<(), String>),
    // Página Alvos de toque (estado local; sem backend ainda).
    TouchLarger(bool),
    TouchMinSize(f32),
    TouchHighlightFocus(bool),
    // Página Contraste (2d.2 + HC parte 2).
    ContrastHigh(bool),
    ContrastReduceTransparency(bool),
    /// Alto contraste lido do compositor (boot + read-on-open, HC parte 2).
    HighContrastLoaded(Result<bool, String>),
    /// Resultado de escrever o alto contraste (HC parte 2); só loga.
    HighContrastWritten(Result<(), String>),
    /// **Sinal** de mudança do alto contraste (HC parte 2). Vem da
    /// Subscription; é a fonte única do re-render — aplica o flag e
    /// re-resolve a paleta ao vivo.
    HighContrastChanged(bool),
    // Página Movimento (2d.2 + 3d).
    MotionReduce(bool),
    MotionAnimSpeed(f32),
    /// Reduzir movimento lido do compositor ao abrir a página (3d).
    MotionReduceLoaded(Result<bool, String>),
    /// Resultado de escrever o reduzir movimento (3d); reverte se falhar.
    MotionReduceWritten(bool, Result<(), String>),
    // Página Sino visual (2d.2).
    BellEnabled(bool),
    BellPhotosafe(bool),
    BellFlashIntensity(f32),
}

pub fn update(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::OpenCategory(label) => {
            state.view = View::Category(label);
            // Read-on-open: lê o estado REAL do compositor pra os toggles
            // iniciarem certos (não com defaults chutados). 3a/3c/3d.
            if let Some(conn) = &state.conn {
                match label {
                    "Sino visual" => {
                        return Task::perform(
                            crate::ipc::read_bell_settings(conn.clone()),
                            Message::BellSettingsLoaded,
                        );
                    }
                    "Movimento" => {
                        return Task::perform(
                            crate::ipc::read_reduce_motion(conn.clone()),
                            Message::MotionReduceLoaded,
                        );
                    }
                    "Contraste" => {
                        return Task::perform(
                            crate::ipc::read_high_contrast(conn.clone()),
                            Message::HighContrastLoaded,
                        );
                    }
                    _ => {}
                }
            }
        }
        Message::Back => state.view = View::Grid,
        Message::SearchChanged(q) => state.query = q,
        Message::Connected(Ok(conn)) => {
            // Lê o alto contraste persistido logo ao conectar, pra o app já
            // ABRIR na paleta certa (HC parte 2). O re-resolve vem pelo
            // mesmo caminho do sinal.
            let read = Task::perform(
                crate::ipc::read_high_contrast(conn.clone()),
                Message::HighContrastLoaded,
            );
            state.conn = Some(conn);
            return read;
        }
        Message::Connected(Err(e)) => {
            eprintln!("luma-config: sem conexão D-Bus com o compositor: {e}");
        }
        Message::BellSettingsLoaded(Ok((enabled, photosafe, intensity))) => {
            state.bell.enabled = enabled;
            state.bell.photosafe = photosafe;
            state.bell.flash_intensity = intensity as f32;
        }
        Message::BellSettingsLoaded(Err(e)) => {
            eprintln!("luma-config: ler settings do sino visual falhou: {e}");
        }
        Message::BellWritten(_, Ok(())) => {}
        Message::BellWritten(attempted, Err(e)) => {
            // Não quebra a UI: loga e reverte o otimismo pro estado anterior.
            eprintln!("luma-config: escrever sino visual falhou: {e}");
            state.bell.enabled = !attempted;
        }
        Message::PhotosafeWritten(_, Ok(())) => {}
        Message::PhotosafeWritten(attempted, Err(e)) => {
            eprintln!("luma-config: escrever photosafe falhou: {e}");
            state.bell.photosafe = !attempted;
        }
        Message::IntensityWritten(Ok(())) => {}
        Message::IntensityWritten(Err(e)) => {
            // Não reverte (re-lê na próxima abertura); só loga.
            eprintln!("luma-config: escrever intensidade falhou: {e}");
        }
        Message::TouchLarger(v) => state.touch.larger = v,
        Message::TouchMinSize(v) => state.touch.min_size = v,
        Message::TouchHighlightFocus(v) => state.touch.highlight_focus = v,
        Message::ContrastHigh(v) => {
            // HC parte 2: o toggle SÓ dispara o set via D-Bus. O re-render
            // (e o state.contrast.high_contrast) vêm do SINAL — fonte única,
            // o app reflete o flag real, não um palpite local.
            if let Some(conn) = &state.conn {
                return Task::perform(
                    crate::ipc::write_high_contrast(conn.clone(), v),
                    Message::HighContrastWritten,
                );
            } else {
                eprintln!("luma-config: sem conexão D-Bus; alto contraste ignorado");
            }
        }
        Message::HighContrastWritten(Ok(())) => {}
        Message::HighContrastWritten(Err(e)) => {
            eprintln!("luma-config: escrever alto contraste falhou: {e}");
        }
        // Boot/read-on-open e o sinal de mudança aplicam o mesmo: setam o
        // flag global, re-resolvem a paleta e re-renderizam ao vivo.
        Message::HighContrastLoaded(Ok(v)) => apply_high_contrast(state, v),
        Message::HighContrastLoaded(Err(e)) => {
            eprintln!("luma-config: ler alto contraste falhou: {e}");
        }
        Message::HighContrastChanged(v) => apply_high_contrast(state, v),
        Message::ContrastReduceTransparency(v) => state.contrast.reduce_transparency = v,
        Message::MotionReduce(v) => {
            // Wiring real (3d): toggle commita na hora (evento único).
            // Otimista + reverte se falhar. Sem conexão: ignora.
            if let Some(conn) = &state.conn {
                state.motion.reduce_motion = v;
                return Task::perform(
                    crate::ipc::write_reduce_motion(conn.clone(), v),
                    move |r| Message::MotionReduceWritten(v, r),
                );
            } else {
                eprintln!("luma-config: sem conexão D-Bus; reduzir movimento ignorado");
            }
        }
        Message::MotionAnimSpeed(v) => state.motion.anim_speed = v,
        Message::MotionReduceLoaded(Ok(v)) => state.motion.reduce_motion = v,
        Message::MotionReduceLoaded(Err(e)) => {
            eprintln!("luma-config: ler reduzir movimento falhou: {e}");
        }
        Message::MotionReduceWritten(_, Ok(())) => {}
        Message::MotionReduceWritten(attempted, Err(e)) => {
            eprintln!("luma-config: escrever reduzir movimento falhou: {e}");
            state.motion.reduce_motion = !attempted;
        }
        Message::BellEnabled(v) => {
            // Wiring real (3a): escreve no compositor. Otimista (mexe a UI
            // já) + reverte se a escrita falhar. Sem conexão: ignora (o
            // toggle volta sozinho, sem backend não há o que ligar).
            if let Some(conn) = &state.conn {
                state.bell.enabled = v;
                return Task::perform(
                    crate::ipc::write_visual_bell(conn.clone(), v),
                    move |r| Message::BellWritten(v, r),
                );
            } else {
                eprintln!("luma-config: sem conexão D-Bus; toggle do sino visual ignorado");
            }
        }
        Message::BellPhotosafe(v) => {
            // Toggle: commita na hora (evento único). Otimista + reverte.
            if let Some(conn) = &state.conn {
                state.bell.photosafe = v;
                return Task::perform(
                    crate::ipc::write_photosafe(conn.clone(), v),
                    move |r| Message::PhotosafeWritten(v, r),
                );
            } else {
                eprintln!("luma-config: sem conexão D-Bus; photosafe ignorado");
            }
        }
        Message::BellFlashIntensity(v) => {
            // Arraste do slider: só atualiza a UI ao vivo. O commit
            // (D-Bus + disco) acontece no soltar — ver BellIntensityReleased.
            state.bell.flash_intensity = v;
        }
        Message::BellIntensityReleased => {
            // Soltou o slider: commita o valor atual. Sem martelar o disco
            // durante o arraste — só este disparo único.
            if let Some(conn) = &state.conn {
                let intensity = state.bell.flash_intensity.round() as u8;
                return Task::perform(
                    crate::ipc::write_flash_intensity(conn.clone(), intensity),
                    Message::IntensityWritten,
                );
            }
        }
    }
    Task::none()
}

/// Theme Golden Hour (iced) — vem do `libluma`, resolvido dos tokens.
pub fn theme(_state: &State) -> Theme {
    // Lê o flag global de contraste → HC ou Golden Hour. Re-resolvido a cada
    // re-render, então acompanha o toggle ao vivo (HC parte 2).
    libluma::golden_hour_theme()
}

/// Aplica o alto contraste ao vivo (HC parte 2): liga o flag global do
/// processo, re-resolve o `GoldenHour` na paleta nova, e guarda no estado
/// pro toggle. O re-render acontece naturalmente após o `update` (o `view`
/// e o `theme` re-resolvem lendo o flag).
fn apply_high_contrast(state: &mut State, on: bool) {
    libluma::tokens::set_high_contrast(on);
    state.gh = libluma::GoldenHour::load();
    state.contrast.high_contrast = on;
}

/// **Subscription** do alto contraste (HC parte 2) — o canal de propagação
/// de aparência. Escuta o sinal `high_contrast_changed` do compositor; cada
/// emissão vira [`Message::HighContrastChanged`] e re-resolve o tema ao
/// vivo. A fase de personalização inteira vai rodar por este canal.
pub fn subscription(_state: &State) -> iced::Subscription<Message> {
    iced::Subscription::run(high_contrast_subscription)
}

/// Builder do stream do sinal (fn pointer = identidade da subscription).
/// Conecta lazy; emite a cada sinal. Se a conexão falhar, fica `pending`
/// (não derruba o app — só não tem HC ao vivo).
fn high_contrast_subscription() -> impl iced::futures::Stream<Item = Message> {
    use iced::futures::StreamExt;
    iced::futures::stream::once(async {
        match crate::ipc::high_contrast_signal_stream().await {
            Ok(s) => s.map(Message::HighContrastChanged).boxed(),
            Err(e) => {
                eprintln!("luma-config: subscription de alto contraste falhou: {e}");
                iced::futures::stream::pending().boxed()
            }
        }
    })
    .flatten()
}

pub fn view(state: &State) -> Element<'_, Message> {
    let gh = state.gh;
    let sem = libluma::tokens::semantic_default();
    let surface = libluma::tokens::to_iced(sem.surface);
    // Borda forte (HC): transparente no normal, #FFF6EA no alto contraste.
    let border = libluma::tokens::to_iced(sem.border);

    responsive(move |size| {
        // Largura confortável do conteúdo (chute ~860px). Abaixo disso
        // enche; no cap, para e centraliza. `side` = metade do excesso
        // (margem dos dois lados), com mínimo pra não colar nas bordas.
        const MIN_PAD: f32 = 32.0;
        const MAX_W: f32 = 860.0;
        let content_w = (size.width - 2.0 * MIN_PAD).clamp(0.0, MAX_W);
        let side = ((size.width - content_w) / 2.0).max(MIN_PAD);

        // O conteúdo depende da view: grade (landing) ou página da categoria.
        let inner: Element<'_, Message> = match state.view {
            View::Grid => {
                let cols = libluma::columns_for(content_w);
                // Busca funcional (2e): text_input estilizado Golden Hour.
                let amber = gh.amber;
                let cream = gh.cream;
                let honey_dim = gh.honey_dim;
                let search = text_input("Buscar configurações…", &state.query)
                    .on_input(Message::SearchChanged)
                    .padding([10, 14])
                    .size(14)
                    .width(Length::Fill)
                    .style(move |_theme, status| {
                        let focused = matches!(status, text_input::Status::Focused { .. });
                        text_input::Style {
                            background: Background::Color(surface),
                            // Foco = âmbar; senão a borda forte do HC
                            // (transparente no normal, então invisível lá).
                            border: Border {
                                radius: 12.0.into(),
                                width: 1.5,
                                color: if focused { amber } else { border },
                            },
                            icon: cream,
                            placeholder: honey_dim,
                            value: cream,
                            selection: Color { a: 0.3, ..amber },
                        }
                    });

                // Filtra a taxonomia pela query (case/accent-insensitive). Query
                // vazia => predicado sempre verdadeiro => grade inteira. Sem
                // match => mensagem no lugar da grade.
                let q = search_fold(&state.query);
                let body: Element<'_, Message> = match libluma::category_grid_filtered::<Message>(
                    &state.taxonomy,
                    cols,
                    move |label| search_fold(label).contains(&q),
                ) {
                    Some(grid) => grid,
                    None => text("Nenhuma configuração encontrada")
                        .size(14)
                        .color(gh.honey_dim)
                        .width(Length::Fill)
                        .center()
                        .into(),
                };
                column(vec![search.into(), body])
                    .spacing(20)
                    .width(Length::Fill)
                    .into()
            }
            View::Category(label) => {
                // Drill-in (2c) + conteúdo (2d). Modelo A: cada categoria
                // tem sua página. Por ora só "Alvos de toque" é real; o
                // resto segue stub até a sua vez.
                let header = libluma::page_header(label, Message::Back);
                let body: Element<'_, Message> = match label {
                    "Alvos de toque" => touch_targets_page(state.touch),
                    "Contraste" => contrast_page(state.contrast),
                    "Movimento" => motion_page(state.motion),
                    "Sino visual" => visual_bell_page(state.bell),
                    _ => text("Conteúdo em breve")
                        .size(14)
                        .color(gh.honey_dim)
                        .into(),
                };
                column(vec![header, body])
                    .spacing(24)
                    .width(Length::Fill)
                    .into()
            }
        };

        // Conteúdo centralizado via padding lateral; o scrollable é a janela
        // inteira (barra na borda direita), estilizado em Golden Hour.
        let centered = container(inner)
            .padding(Padding::from([24.0, side]))
            .width(Length::Fill);
        scrollable(centered)
            .height(Length::Fixed(size.height))
            .style(libluma::scrollbar_style())
            .into()
    })
    .into()
}

/// Página da categoria **Alvos de toque** (2d). Um `SettingsGroup` com as
/// configurações DESSA categoria; estado local (sem backend ainda). As
/// outras configs de acessibilidade (contraste, movimento, sino) moram nas
/// suas próprias páginas, que vêm depois.
fn touch_targets_page(t: TouchTargets) -> Element<'static, Message> {
    libluma::settings_group(
        None,
        vec![
            libluma::toggle_row("Alvos de toque maiores", t.larger, Message::TouchLarger),
            libluma::slider_row(
                "Tamanho mínimo do alvo",
                t.min_size,
                44.0..=64.0,
                1.0,
                "px",
                Message::TouchMinSize,
                None, // local-only (2d); sem commit no backend
            ),
            libluma::toggle_row(
                "Realçar foco do teclado",
                t.highlight_focus,
                Message::TouchHighlightFocus,
            ),
        ],
    )
}

/// Página da categoria **Contraste**. "Alto contraste" é wirado (HC parte
/// 2): o toggle escreve via D-Bus e o app re-resolve a paleta ao vivo pelo
/// sinal de mudança. "Reduzir transparência" segue local (2d.2).
fn contrast_page(c: Contrast) -> Element<'static, Message> {
    libluma::settings_group(
        None,
        vec![
            libluma::toggle_row("Alto contraste", c.high_contrast, Message::ContrastHigh),
            libluma::toggle_row(
                "Reduzir transparência",
                c.reduce_transparency,
                Message::ContrastReduceTransparency,
            ),
        ],
    )
}

/// Página da categoria **Movimento**. "Reduzir movimento" é wirado (3d):
/// lê/escreve o compositor via D-Bus e persiste — ligado, as animações do
/// Luma ficam instantâneas. "Velocidade das animações" segue local (2d.2).
fn motion_page(m: Motion) -> Element<'static, Message> {
    libluma::settings_group(
        None,
        vec![
            libluma::toggle_row("Reduzir movimento", m.reduce_motion, Message::MotionReduce),
            libluma::slider_row(
                "Velocidade das animações",
                m.anim_speed,
                0.0..=200.0,
                25.0,
                "%",
                Message::MotionAnimSpeed,
                None, // local-only (2d.2)
            ),
        ],
    )
}

/// Página da categoria **Sino visual**. Flash da tela no lugar do som.
/// Totalmente wirada (3a/3c): os 3 controles leem/escrevem o compositor via
/// D-Bus e persistem. O slider de intensidade commita no soltar.
fn visual_bell_page(b: VisualBell) -> Element<'static, Message> {
    libluma::settings_group(
        None,
        vec![
            libluma::toggle_row("Sino visual", b.enabled, Message::BellEnabled),
            libluma::toggle_row(
                "Modo seguro para fotossensibilidade",
                b.photosafe,
                Message::BellPhotosafe,
            ),
            libluma::slider_row(
                "Intensidade do flash",
                b.flash_intensity,
                0.0..=100.0,
                10.0,
                "%",
                Message::BellFlashIntensity,
                // Commit no SOLTAR (3c): arraste só mexe a UI; aqui grava.
                Some(Message::BellIntensityReleased),
            ),
        ],
    )
}

/// Fold pra busca (2e): minúsculas (unicode) + remove acento comum (PT-BR),
/// pra "camera" achar "Câmera" e "transferencia" achar "Área de
/// transferência". Aplicado nos dois lados da comparação. Piso é substring
/// case-insensitive; o accent-fold é o que faz a busca parecer esperta.
fn search_fold(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' | 'å' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            other => other,
        })
        .collect()
}

/// Taxonomia travada. Hues resolvidas da paleta Golden Hour; cada
/// categoria mapeia pro seu ícone do pack (`assets/icons/<nome>.svg`,
/// monocromático `currentColor`). O `libluma` recebe o handle e tinge na
/// hue da seção. Enabled hoje: Widgets, Wallpaper e a seção Acessibilidade
/// inteira (Alvos de toque, Contraste, Sino visual, Movimento).
fn build_taxonomy(gh: &GoldenHour) -> Vec<GridSection<'static, Message>> {
    // Handle de svg do pack (bundlado via include_bytes — sem dor de path).
    macro_rules! icon {
        ($name:literal) => {
            svg::Handle::from_memory(
                include_bytes!(concat!("../assets/icons/", $name, ".svg")).as_slice(),
            )
        };
    }
    // Habilitado => clicável (abre a página da categoria); "em breve" => sem
    // clique. O id da categoria é o próprio label.
    let item = |label: &'static str, icon: svg::Handle, enabled: bool| GridItem {
        label,
        icon,
        enabled,
        on_press: if enabled {
            Some(Message::OpenCategory(label))
        } else {
            None
        },
    };
    vec![
        GridSection {
            label: "Aparência",
            hue: gh.amber,
            items: vec![
                item("Tema e cores", icon!("tema-e-cores"), false),
                item("Fontes", icon!("fontes"), false),
                item("Wallpaper", icon!("wallpaper"), true),
                item("Barra de tarefas", icon!("barra-de-tarefas"), false),
                item("Widgets", icon!("widgets"), true),
            ],
        },
        GridSection {
            label: "Acessibilidade",
            hue: gh.sage,
            items: vec![
                item("Alvos de toque", icon!("alvos-de-toque"), true),
                item("Contraste", icon!("contraste"), true),
                item("Sino visual", icon!("sino-visual"), true),
                item("Movimento", icon!("movimento"), true),
            ],
        },
        GridSection {
            label: "Privacidade",
            hue: gh.coral,
            items: vec![
                item("Permissões de tela", icon!("permissoes-de-tela"), false),
                item("Câmera e microfone", icon!("camera-e-microfone"), false),
                item("Área de transferência", icon!("area-de-transferencia"), false),
                item("Telemetria", icon!("telemetria"), false),
            ],
        },
        GridSection {
            label: "Preferências",
            hue: gh.honey,
            items: vec![
                item("Notificações", icon!("notificacoes"), false),
                item("Área de trabalho", icon!("area-de-trabalho"), false),
                item("Tela de bloqueio", icon!("tela-de-bloqueio"), false),
            ],
        },
        GridSection {
            // 2f: abre a taxonomia pra 6 seções — um DE não fecha sem casa
            // pra Tela/Som/Mouse/Teclado/Energia. Tudo "em breve" por agora
            // (páginas vêm depois, uma de cada vez). Rede e Impressoras ainda
            // não entram.
            label: "Dispositivos",
            hue: gh.terracota,
            items: vec![
                item("Tela", icon!("tela"), false),
                item("Som", icon!("som"), false),
                item("Mouse e touchpad", icon!("mouse"), false),
                item("Teclado", icon!("teclado"), false),
                item("Energia", icon!("energia"), false),
                item("Bluetooth", icon!("bluetooth"), false),
            ],
        },
        GridSection {
            label: "Sistema",
            hue: gh.warm_neutral,
            items: vec![
                item("Sobre", icon!("sobre"), false),
                item("Restaurar padrões", icon!("restaurar-padroes"), false),
            ],
        },
    ]
}
