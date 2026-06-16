// Backends de output do Luma.
//
// Cada arquivo aqui representa um backend de execução: aninhado (winit) pra
// desenvolvimento, drm/udev pra rodar nativo. A escolha de qual backend
// inicializar acontece no main com base em variáveis de ambiente.

pub mod winit;
