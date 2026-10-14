//! Sonda da MÁQUINA: o que existe aqui (gestor de pacote, docker, mise) e qual
//! método de instalação já está em uso para cada linguagem.

use super::*;
use crate::ctx::Ctx;

/// Snapshot do que a máquina oferece — calculado uma vez por comando.
pub struct Machine {
    /// Família do SO/distro (decide o gerenciador do método `distro`).
    pub family: Family,
    /// O `mise` está instalado?
    pub mise: bool,
    /// O `docker` está instalado?
    pub docker: bool,
}

impl Machine {
    /// Sonda a máquina (família da distro, mise/docker presentes).
    pub(crate) fn probe(ctx: &Ctx) -> Machine {
        Machine {
            family: ctx.receitas.family(),
            mise: detect::has_mise(ctx),
            docker: detect::has_docker(ctx),
        }
    }

    /// Métodos DISPONÍVEIS nesta máquina, em ordem estável.
    /// mise/official sempre disponíveis (bootstrappáveis); docker só com docker; distro só com família.
    pub(crate) fn available(&self, ctx: &Ctx) -> Vec<Method> {
        Method::ALL.into_iter().filter(|m| self.method_reason(ctx, *m).is_ok()).collect()
    }

    /// Ok se o método é utilizável aqui; Err(razão) caso contrário (deny-by-default). Quem
    /// decide é a [`crate::receitas::Receitas`] do SO.
    pub(crate) fn method_reason(&self, ctx: &Ctx, m: Method) -> Result<(), String> {
        ctx.receitas.method_reason(self, m)
    }
}

/// **O quê:** a regra Unix de métodos disponíveis (deny-by-default).
/// **Onde:** `ReceitasUnix::method_reason`.
pub fn razao_unix(this: &Machine, m: Method) -> Result<(), String> {
    match m {
        // FreeBSD: sem Docker nativo (imagens são Linux) e sem build do mise — recusa honesta
        // aqui evita listar como "disponível" o que o plano depois negaria.
        Method::Docker | Method::Mise if this.family == Family::FreeBsd => {
            Err(tf("env.na_freebsd_method_any", &[("method", m.slug())]))
        }
        Method::Docker if !this.docker => Err("docker não encontrado no PATH.".into()),
        Method::Distro if this.family == Family::Unknown => {
            Err("família da distro não detectada (/etc/os-release).".into())
        }
        _ => Ok(()),
    }
}

/// Como um environment está instalado nesta máquina (pra tabela e idempotência).
pub(crate) fn installed_method(ctx: &Ctx, env: &Env, m: &Machine) -> Option<Method> {
    if m.docker {
        if let Some(img) = defs::docker_image(env.lang) {
            if detect::docker_image_present(img) {
                return Some(Method::Docker);
            }
        }
    }
    if m.mise && detect::mise_has(ctx, defs::mise_tools(env.lang).last().copied().unwrap_or("")) {
        return Some(Method::Mise);
    }
    None
}

/// Idempotência: environment já satisfeito por este método?
pub(crate) fn already_installed(ctx: &Ctx, env: &Env, method: Method, m: &Machine) -> bool {
    match method {
        Method::Docker => {
            defs::docker_image(env.lang).map(detect::docker_image_present).unwrap_or(false)
        }
        Method::Mise => {
            m.mise
                && detect::mise_has(ctx, defs::mise_tools(env.lang).last().copied().unwrap_or(""))
        }
        // distro/official: se o runtime já está no PATH, o objetivo do env está atendido.
        Method::Distro | Method::Official => detect::has_bin(ctx, env.bin),
    }
}
