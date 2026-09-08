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
