use std::time::Duration;

use clap::Parser;
use cosmic::app::{Core, Task};
use cosmic::iced::widget::{self, container, row};
use cosmic::iced::{Alignment, Background, Border, Color, Length};
use cosmic::theme;
use cosmic::widget::toaster::{Toast, ToastId, Toasts, toaster};
use cosmic::widget::{button, column, dialog, icon, nav_bar, text};
use cosmic::{Application, Element};
use luma_ipc::{WidgetInfo, WidgetTarget};

use crate::flags::{CliArgs, LumaConfigFlags};
use crate::ipc;
use crate::pages::Page;

const APP_ID: &str = "dev.luma.Config";

/// Atraso entre tentativas de reconexão quando o compositor não tá no
/// ar. 3s — suficiente pra dev abrir o app antes do compositor e ver o
/// indicador virar sage assim que o `Ping` aceitar.
const RECONNECT_DELAY: Duration = Duration::from_secs(3);

/// Diâmetro do ponto indicador (px) no rodapé.
const DOT_SIZE: u16 = 10;

/// Tamanho do ícone do card de widget.
const CARD_ICON_SIZE: u16 = 48;

/// Largura fixa do card (3c.2 — linha simples; grid responsivo é polish).
const CARD_WIDTH: u16 = 240;

/// Estado da conexão D-Bus com o compositor (3b).
#[derive(Debug, Clone)]
pub enum ConnectionState {
    Probing,
    Connected { version: String },
    Disconnected,
}

/// Estado da página Widgets (3c.2).
#[derive(Debug, Clone)]
pub enum WidgetsState {
    Loading,
    Loaded(Vec<WidgetInfo>),
    Disconnected,
}

/// Contexto de UI do app (3c.4) — NÃO cruza o D-Bus, é puramente
/// estado interno do app. `Panel` e `Desktop` vêm dos itens do menu
/// da Fatia 1 que abrem o app já apontando pra onde o widget vai. Aí
/// o botão Adicionar pula o mini-modal e chama AddWidget direto.
/// `Free` é o estado quando o app é aberto sem contexto (cli sem
/// `--target` ou via "Personalizar desktop") — o mini-modal pergunta.
///
/// Distinto de `luma_ipc::WidgetTarget`, que só tem Panel/Desktop
/// (o que o AddWidget aceita).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppContext {
    Panel,
    Desktop,
    Free,
}

/// Mini-modal de destino (3c.3) — captura qual widget tá sendo
/// adicionado pra renderizar o título "Adicionar X onde?" e fechar
/// quando o destino for escolhido (ou cancelado).
#[derive(Debug, Clone)]
struct AddDialog {
    type_name: String,
    display_name: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    PingResult(Result<String, String>),
    ListResult(Result<Vec<WidgetInfo>, String>),
    /// Botão Adicionar clicado num card — abre o modal.
    OpenAddDialog { type_name: String, display_name: String },
    /// Usuário escolheu destino no modal — dispara AddWidget.
    AddDialogChoose(WidgetTarget),
    /// Usuário cancelou (botão Cancelar do modal).
    AddDialogCancel,
    /// Resultado do AddWidget: `Ok((display_name, target))` em sucesso,
    /// `Err((display_name, error_msg))` em falha.
    AddResult(Result<(String, WidgetTarget), (String, String)>),
    /// Toast expirou ou foi fechado — remove da fila.
    CloseToast(ToastId),
}

pub struct AppModel {
    core: Core,
    nav: nav_bar::Model,
    current_page: Page,
    connection: ConnectionState,
    widgets_state: WidgetsState,
    add_dialog: Option<AddDialog>,
    toasts: Toasts<Message>,
    /// 3c.4: contexto atual (Panel/Desktop pulam o mini-modal).
    app_context: AppContext,
}

impl Application for AppModel {
    type Executor = cosmic::executor::Default;
    type Flags = LumaConfigFlags;
    type Message = Message;

    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(mut core: Core, flags: Self::Flags) -> (Self, Task<Self::Message>) {
        core.set_header_title("Configurações".to_string());

        let mut nav = nav_bar::Model::default();
        nav.insert()
            .text("Widgets")
            .icon(icon::from_name("applications-science-symbolic"))
            .data::<Page>(Page::Widgets)
            .activate();
        nav.insert()
            .text("Wallpaper")
            .icon(icon::from_name("preferences-desktop-wallpaper-symbolic"))
            .data::<Page>(Page::Wallpaper);
        nav.insert()
            .text("Preset")
            .icon(icon::from_name("preferences-desktop-theme-symbolic"))
            .data::<Page>(Page::Preset);

        let mut model = Self {
            core,
            nav,
            current_page: Page::Widgets,
            connection: ConnectionState::Probing,
            widgets_state: WidgetsState::Loading,
            add_dialog: None,
            toasts: Toasts::new(Message::CloseToast),
            app_context: AppContext::Free,
        };
        model.apply_cli_args(&flags.cli);

        (model, ping_task(Duration::ZERO))
    }

    /// Single-instance re-ativação (3c.4): quando alguém dispara
    /// `luma-config --tab ... --target ...` e o app já tá aberto, o
    /// cosmic encaminha os args via D-Bus. Re-parseia e aplica.
    fn dbus_activation(
        &mut self,
        msg: cosmic::dbus_activation::Message,
    ) -> Task<Self::Message> {
        if let cosmic::dbus_activation::Details::ActivateAction { args, .. } = msg.msg {
            // Prepend nome do binário pro clap aceitar o iterator.
            let cli = CliArgs::try_parse_from(
                std::iter::once(String::from("luma-config")).chain(args),
            )
            .unwrap_or_default();
            eprintln!(
                "[luma-config] dbus_activation: tab={:?} target={:?}",
                cli.tab, cli.target
            );
            self.apply_cli_args(&cli);
        }
        Task::none()
    }

    fn nav_model(&self) -> Option<&nav_bar::Model> {
        Some(&self.nav)
    }

    fn on_nav_select(&mut self, id: nav_bar::Id) -> Task<Self::Message> {
        self.nav.activate(id);
        if let Some(page) = self.nav.data::<Page>(id).cloned() {
            self.current_page = page;
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let page: Element<'_, Self::Message> = match self.current_page {
            Page::Widgets => self.view_widgets(),
            Page::Wallpaper => text("Wallpaper — em breve").size(18).into(),
            Page::Preset => text("Preset — em breve").size(18).into(),
        };
        toaster(&self.toasts, page)
    }

    /// Mini-modal de destino (3c.3). cosmic renderiza centralizado
    /// com backdrop escurecido automaticamente.
    fn dialog(&self) -> Option<Element<'_, Self::Message>> {
        let d = self.add_dialog.as_ref()?;
        let title = format!("Adicionar “{}” onde?", d.display_name);
        let panel_btn = button::suggested("Adicionar ao painel")
            .width(Length::Fill)
            .on_press(Message::AddDialogChoose(WidgetTarget::Panel));
        let desktop_btn = button::suggested("Adicionar ao desktop")
            .width(Length::Fill)
            .on_press(Message::AddDialogChoose(WidgetTarget::Desktop));
        let cancel_btn = button::standard("Cancelar")
            .width(Length::Fill)
            .on_press(Message::AddDialogCancel);

        Some(
            dialog()
                .title(title)
                .control(panel_btn)
                .control(desktop_btn)
                .control(cancel_btn)
                .max_width(420.0)
                .into(),
        )
    }

    /// Rodapé: indicador discreto de status da conexão com o compositor.
    fn footer(&self) -> Option<Element<'_, Self::Message>> {
        let colors = &luma_theme::current().colors;
        let dot_color = match &self.connection {
            ConnectionState::Connected { .. } => iced_color(colors.hover_maximize.components()),
            ConnectionState::Disconnected | ConnectionState::Probing => {
                let [r, g, b, _] = colors.hover_minimize.components();
                Color::from_rgba(r, g, b, 0.45)
            }
        };

        let label = match &self.connection {
            ConnectionState::Connected { version } => version.clone(),
            ConnectionState::Disconnected => "Compositor desconectado".to_string(),
            ConnectionState::Probing => "Conectando ao compositor…".to_string(),
        };

        let dot = container(widget::Space::new())
            .width(DOT_SIZE)
            .height(DOT_SIZE)
            .class(theme::Container::custom(move |_| {
                cosmic::iced::widget::container::Style {
                    background: Some(Background::Color(dot_color)),
                    border: Border {
                        radius: (f32::from(DOT_SIZE) / 2.0).into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            }));

        let indicator = row![dot, text(label).size(13)]
            .spacing(8)
            .align_y(Alignment::Center);

        Some(indicator.into())
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::PingResult(Ok(version)) => {
                eprintln!("[luma-config] conectado ao compositor: {version}");
                self.connection = ConnectionState::Connected { version };
                self.widgets_state = WidgetsState::Loading;
                list_task()
            }
            Message::PingResult(Err(err)) => {
                eprintln!(
                    "[luma-config] compositor não respondeu ao Ping ({err}); retry em {}s",
                    RECONNECT_DELAY.as_secs()
                );
                self.connection = ConnectionState::Disconnected;
                self.widgets_state = WidgetsState::Disconnected;
                ping_task(RECONNECT_DELAY)
            }
            Message::ListResult(Ok(widgets)) => {
                eprintln!(
                    "[luma-config] catálogo carregado: {} widget(s)",
                    widgets.len()
                );
                self.widgets_state = WidgetsState::Loaded(widgets);
                Task::none()
            }
            Message::ListResult(Err(err)) => {
                eprintln!(
                    "[luma-config] ListAvailableWidgets falhou: {err} — voltando pro fluxo de retry"
                );
                self.connection = ConnectionState::Disconnected;
                self.widgets_state = WidgetsState::Disconnected;
                ping_task(RECONNECT_DELAY)
            }
            Message::OpenAddDialog { type_name, display_name } => {
                // 3c.4: contexto Panel/Desktop pula o modal e adiciona
                // direto; Free abre o modal (3c.3).
                match context_to_target(self.app_context) {
                    Some(target) => add_widget_task(type_name, display_name, target),
                    None => {
                        self.add_dialog = Some(AddDialog { type_name, display_name });
                        Task::none()
                    }
                }
            }
            Message::AddDialogCancel => {
                self.add_dialog = None;
                Task::none()
            }
            Message::AddDialogChoose(target) => {
                let dialog = match self.add_dialog.take() {
                    Some(d) => d,
                    // Defensive: choose chegou sem modal aberto. Não deveria.
                    None => return Task::none(),
                };
                add_widget_task(dialog.type_name, dialog.display_name, target)
            }
            Message::AddResult(Ok((display_name, target))) => {
                eprintln!(
                    "[luma-config] AddWidget OK: {display_name} → {}",
                    target.as_str()
                );
                let toast_msg =
                    format!("Adicionado: {display_name} no {}", target_label(target));
                // `Toasts::push` devolve iced::Task<Message>; cosmic::Task
                // espera Task<Action<Message>>, daí o map.
                self.toasts.push(Toast::new(toast_msg)).map(cosmic::Action::App)
            }
            Message::AddResult(Err((display_name, err))) => {
                eprintln!("[luma-config] AddWidget falhou ({display_name}): {err}");
                let toast_msg = format!("Falha ao adicionar {display_name}: {err}");
                self.toasts.push(Toast::new(toast_msg)).map(cosmic::Action::App)
            }
            Message::CloseToast(id) => {
                self.toasts.remove(id);
                Task::none()
            }
        }
    }
}

impl AppModel {
    /// Aplica args CLI ao estado: troca aba e contexto. Reutilizado no
    /// boot (`init`) e na re-ativação via single-instance
    /// (`dbus_activation`).
    fn apply_cli_args(&mut self, cli: &CliArgs) {
        let page = match cli.tab.as_deref() {
            Some("wallpaper") => Page::Wallpaper,
            Some("preset") => Page::Preset,
            // Default + valor desconhecido: cai em Widgets (aba que
            // tem mais chance de ser o que o usuário queria — único
            // alvo dos menus da Fatia 1 hoje).
            _ => Page::Widgets,
        };
        self.select_page(page);

        self.app_context = match cli.target.as_deref() {
            Some("panel") => AppContext::Panel,
            Some("desktop") => AppContext::Desktop,
            _ => AppContext::Free,
        };
    }

    /// Troca a aba ativa, casando o `nav_bar` com o `current_page`.
    /// Itera porque o nav_bar não tem lookup por data.
    fn select_page(&mut self, target: Page) {
        let ids: Vec<nav_bar::Id> = self.nav.iter().collect();
        for id in ids {
            if matches!(self.nav.data::<Page>(id), Some(p) if *p == target) {
                self.nav.activate(id);
                break;
            }
        }
        self.current_page = target;
    }

    fn view_widgets(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;

        match &self.widgets_state {
            WidgetsState::Loading => container(text("Carregando catálogo…").size(15))
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into(),
            WidgetsState::Disconnected => container(
                text("Conecte o compositor pra ver os widgets disponíveis.").size(15),
            )
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into(),
            WidgetsState::Loaded(widgets) if widgets.is_empty() => {
                container(text("Nenhum widget disponível.").size(15))
                    .center_x(Length::Fill)
                    .center_y(Length::Fill)
                    .into()
            }
            WidgetsState::Loaded(widgets) => {
                let mut cards = row::with_capacity(widgets.len()).spacing(spacing.space_m);
                for info in widgets {
                    cards = cards.push(card(info));
                }
                container(cards)
                    .padding(spacing.space_m)
                    .width(Length::Fill)
                    .into()
            }
        }
    }
}

fn card(info: &WidgetInfo) -> Element<'_, Message> {
    let spacing = theme::active().cosmic().spacing;

    let icon_widget = icon::from_name(info.icon_name.as_str()).size(CARD_ICON_SIZE);

    let content = column::with_capacity(4)
        .push(icon_widget)
        .push(text::title4(&info.display_name))
        .push(text::body(&info.description))
        .push(
            button::suggested("Adicionar")
                .width(Length::Fill)
                .on_press(Message::OpenAddDialog {
                    type_name: info.type_name.clone(),
                    display_name: info.display_name.clone(),
                }),
        )
        .spacing(spacing.space_xs)
        .align_x(Alignment::Start);

    container(content)
        .padding(spacing.space_s)
        .width(CARD_WIDTH)
        .class(theme::Container::Card)
        .into()
}

fn ping_task(delay: Duration) -> Task<Message> {
    cosmic::task::future(async move {
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        let result = ipc::connect_and_ping()
            .await
            .map_err(|err| err.to_string());
        Message::PingResult(result)
    })
}

fn list_task() -> Task<Message> {
    cosmic::task::future(async {
        let result = ipc::fetch_available_widgets()
            .await
            .map_err(|err| err.to_string());
        Message::ListResult(result)
    })
}

fn iced_color([r, g, b, a]: [f32; 4]) -> Color {
    Color::from_rgba(r, g, b, a)
}

fn target_label(t: WidgetTarget) -> &'static str {
    match t {
        WidgetTarget::Panel => "painel",
        WidgetTarget::Desktop => "desktop",
    }
}

/// 3c.4: mapeia `AppContext` pra `WidgetTarget` quando o destino tá
/// implícito pelo contexto. `Free` retorna None (modal vai perguntar).
fn context_to_target(ctx: AppContext) -> Option<WidgetTarget> {
    match ctx {
        AppContext::Panel => Some(WidgetTarget::Panel),
        AppContext::Desktop => Some(WidgetTarget::Desktop),
        AppContext::Free => None,
    }
}

/// Dispara um `AddWidget` via D-Bus e converte o resultado em
/// `Message::AddResult`. Compartilhado por dois caminhos:
/// (1) o modal escolheu destino (Free + AddDialogChoose), e
/// (2) o contexto Panel/Desktop pula o modal direto (3c.4).
fn add_widget_task(
    type_name: String,
    display_name: String,
    target: WidgetTarget,
) -> Task<Message> {
    let target_str = target.as_str();
    cosmic::task::future(async move {
        match ipc::add_widget(type_name, target_str).await {
            Ok(()) => Message::AddResult(Ok((display_name, target))),
            Err(err) => Message::AddResult(Err((display_name, err.to_string()))),
        }
    })
}
