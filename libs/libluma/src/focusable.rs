//! Widget **focável** custom (Teclado passo 2).
//!
//! O sistema de foco do iced só conhece `text_input`/`text_editor` por
//! padrão: só eles entram na cadeia de `focus_next`/`focus_previous`. Pra a
//! grade de tiles ser navegável por teclado no MESMO fluxo da busca (passo
//! 1), embrulhamos cada tile clicável neste wrapper, que:
//!
//! - implementa a operação [`Focusable`] no `operate` → entra no `focus_next`
//!   junto com a busca, na ordem do widget tree (busca → tiles, linha a
//!   linha);
//! - desenha um **anel de foco** ([`tokens::focus_ring`]) quando focado;
//! - ativa (`on_activate`) com **Enter/Espaço** (quando focado) ou **clique**
//!   — o mesmo que clicar no tile.
//!
//! É a fundação de teclado da grade; o passo 3 estende pros controles da
//! página e o passo 4 compõe o realce forte em cima do anel.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{operation, tree, Operation, Tree, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::{
    keyboard, mouse, Border, Color, Element, Event, Length, Rectangle, Shadow, Size,
};

use crate::tokens;

/// Estado do widget no tree: focado ou não. É isto que a operação de foco do
/// iced lê/escreve, e o `draw` consulta pra desenhar o anel.
#[derive(Default)]
struct State {
    is_focused: bool,
}

impl operation::Focusable for State {
    fn is_focused(&self) -> bool {
        self.is_focused
    }
    fn focus(&mut self) {
        self.is_focused = true;
    }
    fn unfocus(&mut self) {
        self.is_focused = false;
    }
}

/// Wrapper focável de um filho único (o visual do tile). Delega tudo pro
/// conteúdo e adiciona: registro na cadeia de foco, anel quando focado, e
/// ativação por Enter/Espaço/clique.
pub struct Focusable<'a, Message, Theme, Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    on_activate: Message,
    /// Raio do anel de foco (casa com o card do tile).
    ring_radius: f32,
}

/// Embrulha `content` num widget focável que emite `on_activate` ao ser
/// ativado (Enter/Espaço com foco, ou clique). Use só em elementos que
/// DEVEM receber foco — desabilitados ("em breve") não passam por aqui, pra
/// serem pulados na ordem de Tab.
pub fn focusable<'a, Message, Theme, Renderer>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
    on_activate: Message,
) -> Focusable<'a, Message, Theme, Renderer> {
    Focusable {
        content: content.into(),
        on_activate,
        ring_radius: 14.0,
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Focusable<'_, Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: renderer::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        // Registra ESTE tile na cadeia de foco (entra no focus_next junto com
        // a busca). Sem id — a ordem é a do widget tree, que basta.
        let state = tree.state.downcast_mut::<State>();
        operation.focusable(None, layout.bounds(), state);
        // Desce pro conteúdo (sem focáveis hoje, mas mantém geral).
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        // Conteúdo primeiro (é decorativo; não captura nada).
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        if shell.is_event_captured() {
            return;
        }

        let state = tree.state.downcast_mut::<State>();
        match event {
            // Clique = ativar (igual ao mouse_area que isto substitui).
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if cursor.is_over(layout.bounds()) =>
            {
                shell.publish(self.on_activate.clone());
                shell.capture_event();
            }
            // Enter/Espaço com foco = ativar (mesmo que clicar).
            Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) if state.is_focused => {
                use keyboard::key::Named;
                if matches!(
                    key,
                    keyboard::Key::Named(Named::Enter) | keyboard::Key::Named(Named::Space)
                ) {
                    shell.publish(self.on_activate.clone());
                    shell.capture_event();
                    shell.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let inner = self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        );
        if cursor.is_over(layout.bounds()) && matches!(inner, mouse::Interaction::None) {
            mouse::Interaction::Pointer
        } else {
            inner
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );

        // Anel de foco por cima do tile (só quando focado). Borda desenhada
        // por dentro dos bounds (não vaza pro vizinho), cor via token
        // palette-aware (âmbar normal / borda forte no HC).
        let state = tree.state.downcast_ref::<State>();
        if state.is_focused {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: layout.bounds(),
                    border: Border {
                        color: tokens::focus_ring(),
                        width: 2.0,
                        radius: self.ring_radius.into(),
                    },
                    shadow: Shadow::default(),
                    snap: true,
                },
                Color::TRANSPARENT,
            );
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: iced::Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message, Theme, Renderer> From<Focusable<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a + Clone,
    Theme: 'a,
    Renderer: 'a + renderer::Renderer,
{
    fn from(w: Focusable<'a, Message, Theme, Renderer>) -> Self {
        Element::new(w)
    }
}
