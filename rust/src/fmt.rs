//! Formatacao e os mostradores pequenos: barra, faisca, linha de metrica.

use ratatui::style::{Color, Style};
use ratatui::text::Span;

use crate::cfg::{AMBAR, BLOCOS, FOSCO, LARANJA, LARANJA_FORTE};

pub fn bytes_h(n: f64) -> String {
    let mut v = n;
    for u in ["", "K", "M", "G", "T"] {
        if v.abs() < 1024.0 {
            return if u.is_empty() {
                format!("{:.0}B", v)
            } else {
                format!("{:.1}{}B", v, u)
            };
        }
        v /= 1024.0;
    }
    format!("{:.1}PB", v)
}

pub fn dur_h(seg: f64) -> String {
    let s = seg.max(0.0) as u64;
    let (d, resto) = (s / 86400, s % 86400);
    let (h, resto) = (resto / 3600, resto % 3600);
    let m = resto / 60;
    if d > 0 {
        format!("{}d {:02}h", d, h)
    } else {
        format!("{:02}h {:02}m", h, m)
    }
}

/// Vermelho e exclusivo de indisponibilidade total; um disco em 92% e grave
/// mas a maquina esta de pe, entao o topo da escala e laranja forte.
pub fn cor_por_valor(pct: f64) -> Color {
    if pct >= 90.0 {
        LARANJA_FORTE
    } else if pct >= 70.0 {
        LARANJA
    } else {
        AMBAR
    }
}

pub fn barra<'a>(pct: f64, largura: usize) -> Vec<Span<'a>> {
    let pct = pct.clamp(0.0, 100.0);
    let cheio = ((pct / 100.0 * largura as f64).round() as usize).min(largura);
    vec![
        Span::styled("█".repeat(cheio), Style::new().fg(cor_por_valor(pct))),
        Span::styled("░".repeat(largura - cheio), Style::new().fg(FOSCO)),
    ]
}

/// Grafico de linha em blocos.
///
/// `teto` fixo desenha em escala absoluta, mas com raiz quadrada: no linear,
/// utilizacao baixa (cpu a 1%) arredondava pro nivel zero, que e espaco em
/// branco, e o grafico sumia. Sem teto, escala pelo proprio pico com 30% de
/// folga, senao a linha fica sempre no talo e vira uma barra solida.
/// Valor maior que zero nunca renderiza vazio.
pub fn faisca<'a>(valores: &[f64], largura: usize, cor: Color, teto: Option<f64>) -> Vec<Span<'a>> {
    if valores.is_empty() {
        return vec![Span::styled(
            "░".repeat(largura),
            Style::new().fg(FOSCO),
        )];
    }
    let dados: &[f64] = if valores.len() > largura {
        &valores[valores.len() - largura..]
    } else {
        valores
    };
    let comprime = teto.is_some();
    let pico = dados.iter().cloned().fold(0.0_f64, f64::max);
    let escala = match teto {
        Some(t) => t,
        None => {
            let e = pico * 1.3;
            if e > 0.0 {
                e
            } else {
                1.0
            }
        }
    };
    let escala = if escala > 0.0 { escala } else { 1.0 };

    let mut spans = Vec::new();
    if dados.len() < largura {
        spans.push(Span::styled(
            "░".repeat(largura - dados.len()),
            Style::new().fg(FOSCO),
        ));
    }
    let mut buf = String::new();
    for v in dados {
        let idx = if *v <= 0.0 {
            0
        } else {
            let mut frac = (v / escala).min(1.0);
            if comprime {
                frac = frac.sqrt();
            }
            ((frac * 8.0).round() as usize).clamp(1, 8)
        };
        buf.push(BLOCOS[idx]);
    }
    spans.push(Span::styled(buf, Style::new().fg(cor)));
    spans
}

pub fn linha_metrica<'a>(rotulo: &str, valor: &str, pct: f64, largura: usize) -> Vec<Span<'a>> {
    let mut spans = vec![Span::styled(
        format!("{:<5} ", rotulo),
        Style::new().fg(FOSCO),
    )];
    spans.extend(barra(pct, largura));
    spans.push(Span::styled(
        format!(" {:>7}", valor),
        Style::new().fg(cor_por_valor(pct)),
    ));
    spans
}
