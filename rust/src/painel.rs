//! O diagrama da aba DIAGRAMA, no estilo da tela do MAGI: tres blocos
//! preenchidos com o nome vazado em preto, cantos chanfrados apontando pro
//! centro, e MAGI no meio. A cor do bloco e o estado da unidade, entao a tela
//! inteira muda de cor junto, que e o efeito do "ALL GREEN" do original.
//!
//! O chanfro usa bloco de tres quadrantes e nao triangulo geometrico: o
//! triangulo nao preenche a celula na maioria das fontes e a diagonal sai
//! pontilhada, enquanto o quadrante fecha solido.

use std::sync::{Arc, Mutex};

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::cfg::{FOSCO, LARANJA, LARANJA_FORTE, PRETO};
use crate::coleta::Unidade;
use crate::fmt::dur_h;

/// Falta o quadrante do canto cortado: tl perde o superior esquerdo, e assim
/// por diante.
const CHANFRO_TL: char = '▟';
const CHANFRO_TR: char = '▙';
const CHANFRO_BL: char = '▜';
const CHANFRO_BR: char = '▛';

#[derive(Clone, Copy)]
struct Celula {
    ch: char,
    fg: Option<Color>,
    bg: Option<Color>,
}

impl Default for Celula {
    fn default() -> Self {
        Celula {
            ch: ' ',
            fg: None,
            bg: None,
        }
    }
}

struct Tela {
    largura: usize,
    altura: usize,
    celulas: Vec<Celula>,
}

impl Tela {
    fn nova(largura: usize, altura: usize) -> Self {
        Tela {
            largura,
            altura,
            celulas: vec![Celula::default(); largura * altura],
        }
    }

    fn em(&mut self, r: usize, c: usize) -> Option<&mut Celula> {
        if r < self.altura && c < self.largura {
            Some(&mut self.celulas[r * self.largura + c])
        } else {
            None
        }
    }

    fn le(&self, r: usize, c: usize) -> Celula {
        if r < self.altura && c < self.largura {
            self.celulas[r * self.largura + c]
        } else {
            Celula::default()
        }
    }

    fn linhas<'a>(&self) -> Vec<Line<'a>> {
        let mut saida = Vec::with_capacity(self.altura);
        for r in 0..self.altura {
            let mut spans: Vec<Span> = Vec::new();
            for c in 0..self.largura {
                let cel = self.le(r, c);
                let mut estilo = Style::new();
                if let Some(f) = cel.fg {
                    estilo = estilo.fg(f);
                }
                if let Some(b) = cel.bg {
                    estilo = estilo.bg(b);
                }
                spans.push(Span::styled(cel.ch.to_string(), estilo));
            }
            saida.push(Line::from(spans));
        }
        saida
    }
}

#[derive(Clone, Copy)]
struct Cortes {
    tl: usize,
    tr: usize,
    bl: usize,
    br: usize,
}

impl Cortes {
    const NENHUM: Cortes = Cortes {
        tl: 0,
        tr: 0,
        bl: 0,
        br: 0,
    };
}

/// Bloco preenchido com o texto vazado, cantos chanfrados.
///
/// O chanfro anda uma coluna por linha: com duas, o degrau fica com duas
/// colunas e o bisel ocupa so a primeira, entao o corte parece grosseiro.
/// Linha com corte zero e linha cheia — pintar o bisel ali deixava uma lasca
/// solta na borda, que era o que fazia o bloco parecer serrilhado.
#[allow(clippy::too_many_arguments)]
fn bloco(
    tela: &mut Tela,
    r0: usize,
    c0: usize,
    largura: usize,
    altura: usize,
    linhas: &[String],
    cor: Color,
    cortes: Cortes,
) {
    for i in 0..altura {
        let de_baixo = altura - 1 - i;
        for j in 0..largura {
            let mut diag: Option<char> = None;
            let mut vazio = false;

            let lados: [(char, bool, usize, bool); 4] = [
                (CHANFRO_TL, i < cortes.tl, cortes.tl.saturating_sub(1 + i), true),
                (
                    CHANFRO_BL,
                    de_baixo < cortes.bl,
                    cortes.bl.saturating_sub(1 + de_baixo),
                    true,
                ),
                (CHANFRO_TR, i < cortes.tr, cortes.tr.saturating_sub(1 + i), false),
                (
                    CHANFRO_BR,
                    de_baixo < cortes.br,
                    cortes.br.saturating_sub(1 + de_baixo),
                    false,
                ),
            ];
            for (ch, ativo, corte, esquerda) in lados {
                if !ativo || corte == 0 {
                    continue;
                }
                if esquerda {
                    if j < corte {
                        vazio = true;
                    } else if j == corte {
                        diag = Some(ch);
                    }
                } else {
                    let lim = largura - 1 - corte;
                    if j > lim {
                        vazio = true;
                    } else if j == lim {
                        diag = Some(ch);
                    }
                }
            }

            if let Some(cel) = tela.em(r0 + i, c0 + j) {
                if let Some(d) = diag {
                    cel.ch = d;
                    cel.fg = Some(cor);
                } else if !vazio {
                    cel.ch = ' ';
                    cel.bg = Some(cor);
                }
            }
        }
    }

    // A altura do texto e escolhida onde o bloco e mais largo, e ele
    // centraliza no trecho que a linha tem preenchido, nao no retangulo
    // nominal: sem isso a frase encosta no bisel nas linhas chanfradas.
    if linhas.is_empty() || altura < linhas.len() {
        return;
    }
    let vao_em = |tela: &Tela, linha: usize| -> (usize, usize) {
        let cheias: Vec<usize> = (0..largura)
            .filter(|j| tela.le(linha, c0 + j).bg == Some(cor))
            .collect();
        match (cheias.first(), cheias.last()) {
            (Some(a), Some(b)) => (*a, b - a + 1),
            _ => (0, 0),
        }
    };
    let meio = (altura - linhas.len()) / 2;
    let topo = (0..=(altura - linhas.len()))
        .max_by_key(|o| {
            let menor = (0..linhas.len())
                .map(|n| vao_em(tela, r0 + o + n).1)
                .min()
                .unwrap_or(0);
            (menor, usize::MAX - o.abs_diff(meio))
        })
        .unwrap_or(meio);

    for (n, texto) in linhas.iter().enumerate() {
        let linha = r0 + topo + n;
        let (esq, vao) = vao_em(tela, linha);
        if vao == 0 {
            continue;
        }
        let chars: Vec<char> = texto.chars().take(vao).collect();
        let ini = c0 + esq + (vao - chars.len()) / 2;
        for (k, ch) in chars.into_iter().enumerate() {
            if let Some(cel) = tela.em(linha, ini + k) {
                cel.ch = ch;
                cel.fg = Some(PRETO);
            }
        }
    }
}

fn liga(tela: &mut Tela, r: usize, c: usize, comp: usize, cor: Color) {
    for i in 0..comp {
        if let Some(cel) = tela.em(r, c + i) {
            cel.ch = ' ';
            cel.bg = Some(cor);
        }
    }
}

/// Limpa o fundo tambem: sem isso o texto cai por cima de uma ligacao e sai
/// laranja sobre laranja, ou seja, invisivel.
fn rotulo(tela: &mut Tela, r: usize, c: usize, texto: &str, cor: Color) {
    for (i, ch) in texto.chars().enumerate() {
        if let Some(cel) = tela.em(r, c + i) {
            cel.ch = ch;
            cel.fg = Some(cor);
            cel.bg = None;
        }
    }
}

/// As tres linhas que vao dentro do bloco: quem e, o que esta pegando, e o
/// que carrega. A segunda linha e o gargalo, pra tela dizer o que esta
/// causando o problema e nao so que ha um.
fn ficha(u: &Unidade) -> Vec<String> {
    if !u.online {
        return vec![u.rotulo(), "SEM RESPOSTA".into(), u.ip.clone()];
    }
    let (estado, _) = u.estado();
    let (recurso, valor) = u.gargalo();
    vec![
        u.rotulo(),
        format!("{} · {} {:.0}%", estado, recurso, valor),
        format!(
            "{} ctn · {} ten · {}",
            u.containers.len(),
            u.tenants(),
            dur_h(u.uptime)
        ),
    ]
}

/// As tres unidades em triangulo em volta do centro.
pub fn painel_unidades<'a>(unidades: &[Arc<Mutex<Unidade>>]) -> Vec<Line<'a>> {
    let peca = |nome: &str| -> (Vec<String>, Color) {
        for u in unidades {
            let u = u.lock().unwrap();
            if u.nome == nome {
                return (ficha(&u), u.estado().1);
            }
        }
        (vec![nome.to_string(), "-".into(), "-".into()], FOSCO)
    };

    // A tela tem a largura exata do desenho: sobrando coluna, o centralizador
    // centraliza a moldura e o conjunto fica deslocado dentro dela.
    let mut tela = Tela::nova(64, 16);

    let (linhas, cor) = peca("BALTHASAR");
    bloco(
        &mut tela,
        0,
        17,
        30,
        6,
        &linhas,
        cor,
        Cortes {
            bl: 4,
            br: 4,
            ..Cortes::NENHUM
        },
    );
    let (linhas, cor) = peca("CASPER");
    bloco(
        &mut tela,
        9,
        0,
        30,
        7,
        &linhas,
        cor,
        Cortes {
            tr: 5,
            ..Cortes::NENHUM
        },
    );
    let (linhas, cor) = peca("MELCHIOR");
    bloco(
        &mut tela,
        9,
        34,
        30,
        7,
        &linhas,
        cor,
        Cortes {
            tl: 5,
            ..Cortes::NENHUM
        },
    );

    // As barras encostam onde o chanfro de fato comeca, nao na borda nominal,
    // senao ficam boiando no vao sem ligar nada.
    liga(&mut tela, 6, 29, 6, LARANJA);
    liga(&mut tela, 8, 24, 4, LARANJA);
    liga(&mut tela, 8, 36, 4, LARANJA);
    rotulo(&mut tela, 7, 28, "M A G I", LARANJA_FORTE);

    tela.linhas()
}
