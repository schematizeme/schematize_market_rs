//! **schematize market** — instalar e remover programas. Só isso, e bem.
//!
//! **O quê:** [`environments`] instala linguagens (docker | mise | distro | official) e
//! ferramentas de dev; [`appsdacasa`] descobre e instala os apps do ecossistema
//! (deployer, optimizer, skills, overdev).
//!
//! **Onde:** app próprio (ADR-0012), extraído do hub.
//!
//! ## Por que o `env` veio junto, e não só os apps da casa
//!
//! O pedido foi "um app focado em instalação e deleção de programas" — e o CLI já tinha
//! **dois** comandos que faziam isso: o `env` (linguagens) e o `apps` (apps da casa). Um
//! gestor de programas que não instala linguagem seria metade de produto, e deixaria o hub
//! com exatamente a responsabilidade que o pedido queria tirar dele.
//!
//! Como efeito, o pedido de *"deployer e optimizer aparecerem no env"* deixou de existir:
//! não há duas listas para reconciliar, há uma.

pub mod appsdacasa;
pub mod environments;
pub mod nucleo;

// Os módulos movidos chamam `crate::util::…` e `crate::agentrun::…`. Estes aliases mantêm o
// código IDÊNTICO ao que era no hub — o corte é `/eng-refactor`, e reescrever 2.700 linhas de
// chamada seria mudança de comportamento disfarçada de mudança de caminho.
pub use nucleo::i18n;
pub use nucleo::util;
pub mod agentrun {
    pub use crate::nucleo::bin::{abrir_comando_no_terminal, binary_in_path, resolve_bin};
}
