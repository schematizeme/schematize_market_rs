//! Ícone do **Market** desenhado EM CÓDIGO.
//!
//! **Por que é cópia do gerador do schematize, com outra paleta:** os três apps da casa
//! precisam parecer **da mesma família** no menu de aplicativos — mesmo squircle, mesma
//! construção — e ao mesmo tempo ser **distinguíveis à distância**, que é como se escolhe
//! ícone. Compartilhar o desenho e trocar cor e disposição dá as duas coisas; um ícone
//! genérico não daria nenhuma.
//!
//! Market: acento **roxo**, três nós EMPILHADOS — a forma de uma prateleira, que é o que um
//! catálogo de programas é. Distingue-se do Deployer (cadeia diagonal) e do Optimizer
//! (escada decrescente) pela FORMA, não só pela cor — e isso sobrevive a 16px e a daltonismo.
//!
//! Ícone do app desenhado EM CÓDIGO (RGBA) — sem crate de imagem nem asset externo, então é
//! RESILIENTE: nunca "some" nem quebra o build por falta de arquivo, e sai em qualquer tamanho.
//! O quê: a marca schematize — squircle escuro da casa + grafo de 3 nós no acento (nó de topo mais
//! claro), o MESMO visual do `assets/icons/schematize.svg` / hicolor / .desktop. Onde: ícone da
//! janela/taskbar da GUI (via `make_app_icon`) e notificações. Antialiasing por supersampling, então
//! fica nítido de 16px a 1024px sem depender de rasterizador externo.

// Paleta = a mesma do Theme/SVG (fundo escuro, acento azul, nó de topo claro).
const BG_TL: [f32; 3] = [0x14 as f32, 0x16 as f32, 0x1c as f32]; // #14161c (canto sup-esq)
const BG_BR: [f32; 3] = [0x1b as f32, 0x1e as f32, 0x27 as f32]; // #1b1e27 (canto inf-dir)
const ACCENT: [f32; 3] = [0x9b as f32, 0x7c as f32, 0xf0 as f32]; // #9b7cf0 roxo (Market)
const ACCENT_HI: [f32; 3] = [0xc4 as f32, 0xb0 as f32, 0xff as f32]; // #c4b0ff (o de cima)
const RING: [f32; 3] = [0x14 as f32, 0x16 as f32, 0x1c as f32]; // anel escuro separando nó da aresta

/// Amostras por eixo no supersampling (SS×SS por pixel) → bordas suaves sem lib de imagem.
const SS: u32 = 4;

/// Gera o ícone em RGBA (com alpha) no tamanho `n` (px). Retorna (bytes, w, h). Puro/determinístico.
pub fn rgba(n: u32) -> (Vec<u8>, u32, u32) {
    let nf = n as f32;
    // Geometria em FRAÇÃO do lado (batendo com o SVG /1024): squircle + triângulo de 3 nós.
    let radius = nf * 0.227; // rx=232/1024
                             // PRATELEIRA: três nós empilhados na vertical. É o que um catálogo é.
    let nodes = [
        (0.500_f32, 0.740_f32, ACCENT),    // base
        (0.500_f32, 0.500_f32, ACCENT),    // meio
        (0.500_f32, 0.260_f32, ACCENT_HI), // topo (destacado)
    ];
    let edges = [(0usize, 1usize), (1, 2)];
    let node_r = nf * 0.090;
    let ring_w = nf * 0.018; // anel escuro ao redor do nó
    let edge_w = nf * 0.047; // stroke 48/1024

    let mut buf = vec![0u8; (n * n * 4) as usize];
    let inv_ss2 = 1.0 / (SS * SS) as f32;

    for y in 0..n {
        for x in 0..n {
            let (mut r, mut g, mut b, mut a) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            // Supersampling: SS×SS subpixels, média (cobertura + cor).
            for sy in 0..SS {
                for sx in 0..SS {
                    let px = x as f32 + (sx as f32 + 0.5) / SS as f32;
                    let py = y as f32 + (sy as f32 + 0.5) / SS as f32;
                    if let Some(c) =
                        sample(px, py, nf, radius, &nodes, &edges, node_r, ring_w, edge_w)
                    {
                        r += c[0];
                        g += c[1];
                        b += c[2];
                        a += 255.0;
                    }
                }
            }
            let idx = ((y * n + x) * 4) as usize;
            let cover = a * inv_ss2; // 0..255 (alpha final = cobertura)
            if cover > 0.0 {
                // média das cores só sobre os subpixels COBERTOS (evita halo escuro na borda).
                let covered = a / 255.0;
                buf[idx] = (r / covered).round().clamp(0.0, 255.0) as u8;
                buf[idx + 1] = (g / covered).round().clamp(0.0, 255.0) as u8;
                buf[idx + 2] = (b / covered).round().clamp(0.0, 255.0) as u8;
                buf[idx + 3] = cover.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    (buf, n, n)
}

/// Cor OPACA de um subpixel (ou `None` se fora do squircle → transparente). Ordem de pintura:
/// fundo (gradiente) → arestas (acento) → anel do nó (escuro) → miolo do nó (acento/claro).
#[allow(clippy::too_many_arguments)]
fn sample(
    px: f32,
    py: f32,
    nf: f32,
    radius: f32,
    nodes: &[(f32, f32, [f32; 3])],
    edges: &[(usize, usize)],
    node_r: f32,
    ring_w: f32,
    edge_w: f32,
) -> Option<[f32; 3]> {
    if !inside_rounded(px, py, nf, radius) {
        return None;
    }
    // Fundo: gradiente diagonal TL→BR.
    let t = ((px + py) / (2.0 * nf)).clamp(0.0, 1.0);
    let mut color = [
        BG_TL[0] + (BG_BR[0] - BG_TL[0]) * t,
        BG_TL[1] + (BG_BR[1] - BG_TL[1]) * t,
        BG_TL[2] + (BG_BR[2] - BG_TL[2]) * t,
    ];
    // Arestas (acento).
    for &(a, b) in edges {
        let p1 = (nodes[a].0 * nf, nodes[a].1 * nf);
        let p2 = (nodes[b].0 * nf, nodes[b].1 * nf);
        if dist_to_segment(px, py, p1, p2) <= edge_w * 0.5 {
            color = ACCENT;
            break;
        }
    }
    // Nós: anel escuro (raio node_r+ring_w) e miolo colorido (raio node_r).
    for &(cx, cy, node_color) in nodes {
        let dx = px - cx * nf;
        let dy = py - cy * nf;
        let d = (dx * dx + dy * dy).sqrt();
        if d <= node_r + ring_w {
            color = if d <= node_r { node_color } else { RING };
            break;
        }
    }
    Some(color)
}

/// **O quê:** o PNG RGBA (codificado) do ícone em `n`×`n` px, gerado A PARTIR DO CÓDIGO.
/// **Onde:** [`icones_hicolor`] — o market desenha o ícone e ENTREGA os bytes à costura
/// `plat.atalhos`, que decide onde gravar (hicolor no Unix; `.ico`/Start Menu no Windows).
pub fn png_bytes(n: u32) -> std::io::Result<Vec<u8>> {
    let (data, w, h) = rgba(n);
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(std::io::Error::other)?;
    writer.write_image_data(&data).map_err(std::io::Error::other)?;
    drop(writer);
    Ok(out)
}

/// Tamanhos hicolor padrão (freedesktop).
pub const HICOLOR_SIZES: [u32; 8] = [16, 24, 32, 48, 64, 128, 256, 512];

/// **O quê:** o ícone em TODOS os tamanhos hicolor, como `(px, bytes PNG)`.
/// **Onde:** `EntradaAtalho::icones` em [`crate::nucleo::desktop::entrada`]; a costura grava
/// cada um em `hicolor/<n>x<n>/apps/<icone_nome>.png`.
pub fn icones_hicolor() -> std::io::Result<Vec<(u32, Vec<u8>)>> {
    HICOLOR_SIZES.iter().map(|&n| png_bytes(n).map(|b| (n, b))).collect()
}

/// Ponto dentro de um quadrado (0..n) com cantos de raio `r` (squircle aproximado por raio).
fn inside_rounded(px: f32, py: f32, n: f32, r: f32) -> bool {
    if px < 0.0 || py < 0.0 || px > n || py > n {
        return false;
    }
    let cx = px.clamp(r, n - r);
    let cy = py.clamp(r, n - r);
    let dx = px - cx;
    let dy = py - cy;
    dx * dx + dy * dy <= r * r
}

/// Distância de um ponto ao segmento p1-p2.
fn dist_to_segment(px: f32, py: f32, p1: (f32, f32), p2: (f32, f32)) -> f32 {
    let (x1, y1) = p1;
    let (x2, y2) = p2;
    let dx = x2 - x1;
    let dy = y2 - y1;
    let len2 = dx * dx + dy * dy;
    if len2 == 0.0 {
        return ((px - x1).powi(2) + (py - y1).powi(2)).sqrt();
    }
    let t = (((px - x1) * dx + (py - y1) * dy) / len2).clamp(0.0, 1.0);
    let projx = x1 + t * dx;
    let projy = y1 + t * dy;
    ((px - projx).powi(2) + (py - projy).powi(2)).sqrt()
}
