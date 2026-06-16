//! Contratos D-Bus entre compositor (servidor) e app (cliente). DTOs
//! com derives de serde + zbus::zvariant::Type; tipos de domínio que
//! NÃO cruzam D-Bus diretamente (ex.: `WidgetTarget`) também moram aqui
//! pra os dois lados converterem do/pro wire format com a mesma lógica.

use serde::{Deserialize, Serialize};
use zvariant::Type;

/// Info de um tipo de widget disponível no compositor. Usado pelo
/// `ListAvailableWidgets` pra alimentar o picker do app.
///
/// Wire signature (zbus derive): `(ssss)`. `Vec<WidgetInfo>` vira
/// `a(ssss)`.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct WidgetInfo {
    /// Identificador do tipo no registry (ex.: "label", "counter").
    /// É o que volta no `AddWidget`.
    pub type_name: String,
    /// Nome humano pro picker.
    pub display_name: String,
    /// Frase curta com o que o widget faz.
    pub description: String,
    /// Nome do ícone no icon theme freedesktop-symbolic.
    pub icon_name: String,
}

/// Destino de um widget ao adicionar. NÃO cruza o D-Bus como enum —
/// cruza como String, conversão acontece nas pontas via
/// `as_str`/`from_str`. Decisão: strings ficam legíveis no busctl/
/// gdbus, sem dor de serialização de enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetTarget {
    Panel,
    Desktop,
}

impl WidgetTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            WidgetTarget::Panel => "panel",
            WidgetTarget::Desktop => "desktop",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "panel" => Some(WidgetTarget::Panel),
            "desktop" => Some(WidgetTarget::Desktop),
            _ => None,
        }
    }
}
