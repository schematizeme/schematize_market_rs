//! **Ctx** — o par `(Plataforma, Receitas)` que desce por todo o market.
//!
//! **O quê:** agrupa as costuras de plataforma (`&Plataforma`, do `schematize_plataforma`) e as
//! receitas de instalação (`&dyn Receitas`) numa referência só, para os dois descerem juntos
//! pelas camadas (`cli` -> `atualizar`/`environments` -> `nucleo`) sem globais (ADR-0026).
//!
//! **Onde:** montado uma vez em [`crate::cli::run`]; os testes usam [`Ctx::de_teste`].

use crate::receitas::Receitas;
use schematize_plataforma::seams::Plataforma;

/// Referências às costuras do SO e às receitas de instalação.
#[derive(Clone, Copy)]
pub struct Ctx<'a> {
    /// Costuras (shell, rede, substituição de binário, atalhos, PATH, terminal, abridor).
    pub plat: &'a Plataforma,
    /// Receitas de instalação do SO (Unix aqui; Windows no repo `schematize_windows_rs`).
    pub receitas: &'a dyn Receitas,
}

impl<'a> Ctx<'a> {
    /// **O quê:** monta o contexto a partir das duas referências.
    /// **Onde:** [`crate::cli::run`].
    pub fn novo(plat: &'a Plataforma, receitas: &'a dyn Receitas) -> Self {
        Ctx { plat, receitas }
    }
}

#[cfg(all(test, unix))]
impl Ctx<'static> {
    /// **O quê:** contexto Unix real para os testes (a `Plataforma` e as receitas vivem num
    /// `static` preguiçoso). **Onde:** os testes do crate que tocam o disco/processo real.
    pub fn de_teste() -> Ctx<'static> {
        use std::sync::OnceLock;
        static PLAT: OnceLock<Plataforma> = OnceLock::new();
        static REC: crate::receitas_unix::ReceitasUnix = crate::receitas_unix::ReceitasUnix;
        Ctx { plat: PLAT.get_or_init(Plataforma::unix), receitas: &REC }
    }
}
