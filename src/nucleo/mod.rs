//! NÚCLEO — a infraestrutura mínima para o market andar sozinho.
//!
//! **Por que é cópia:** o mesmo motivo do Deployer e do Optimizer (ADR-0010/0011/0012) —
//! depender do crate do hub amarraria os apps e mataria a propriedade que os justifica:
//! instalar e funcionar sozinho. Nada de domínio entra aqui.

pub mod bin;
pub mod config;
pub mod desktop;
pub mod i18n;
pub mod icone;
pub mod util;

// O caminho de ATUALIZAÇÃO, herdado do `schematize_updater_rs` (ADR-0013): o que o SO é
// (`plataforma`), o que se instala nele (`toolchain`) e como se fala com a rede (`rede`).
// Ficam no núcleo, e não em `atualizar/`, porque não são do domínio de atualizar — são a
// infraestrutura que qualquer caminho do market usa para saber onde está e o que baixar.
pub mod plataforma;
pub mod rede;
pub mod toolchain;
