//! As abas: GERAL, uma por unidade, e DIAGRAMA.

use std::sync::{Arc, Mutex};

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Padding, Paragraph};
use ratatui::Frame;

use crate::cfg::{self, AMBAR, CIANO, FOSCO, LARANJA, TETO_MEM, VERDE, VERMELHO};
use crate::coleta::Unidade;
use crate::fmt::{barra, bytes_h, cor_por_valor, dur_h, faisca, linha_metrica};
use crate::logo;
use crate::painel::painel_unidades;

fn fosco<'a>(s: impl Into<String>) -> Span<'a> {
    Span::styled(s.into(), Style::new().fg(FOSCO))
}


// ------------------------------------------------------------- tabelas ---
//
// O `Table` do ratatui nao desenha separador entre colunas e estica pra
// altura toda que recebe. A versao em Python usa a caixa padrao do rich, que
// tem cabecalho pesado e regua entre as colunas — entao aqui a tabela e
// montada a mao, pras duas versoes ficarem iguais.

pub struct Cel<'a> {
    pub spans: Vec<Span<'a>>,
    /// Largura visual do conteudo, contada por quem monta: um span pode ter
    /// mais bytes que colunas.
    pub largura: usize,
    pub direita: bool,
}

impl<'a> Cel<'a> {
    pub fn txt(s: impl Into<String>, estilo: Style, direita: bool) -> Cel<'a> {
        let s = s.into();
        let largura = s.chars().count();
        Cel {
            spans: vec![Span::styled(s, estilo)],
            largura,
            direita,
        }
    }

    pub fn spans(spans: Vec<Span<'a>>, largura: usize) -> Cel<'a> {
        Cel {
            spans,
            largura,
            direita: false,
        }
    }
}

fn preenche<'a>(cel: &Cel<'a>, largura: usize) -> Vec<Span<'a>> {
    let folga = largura.saturating_sub(cel.largura);
    let (antes, depois) = if cel.direita { (folga, 0) } else { (0, folga) };
    let mut v = Vec::with_capacity(cel.spans.len() + 2);
    if antes > 0 {
        v.push(Span::raw(" ".repeat(antes)));
    }
    v.extend(cel.spans.iter().cloned());
    if depois > 0 {
        v.push(Span::raw(" ".repeat(depois)));
    }
    v
}

fn regua<'a>(larguras: &[usize], esq: &str, meio: &str, dir: &str, traco: &str) -> Line<'a> {
    let mut s = String::from(esq);
    for (i, l) in larguras.iter().enumerate() {
        s.push_str(&traco.repeat(l + 2));
        s.push_str(if i + 1 == larguras.len() { dir } else { meio });
    }
    Line::from(Span::styled(s, Style::new().fg(FOSCO)))
}

/// Monta a tabela inteira em linhas. `larguras` fixa cada coluna; a de indice
/// `flexivel` recebe a sobra pra tabela ocupar a largura pedida.
pub fn tabela<'a>(
    cabecalho: &[&str],
    larguras: &mut [usize],
    flexivel: usize,
    linhas: Vec<Vec<Cel<'a>>>,
    largura_total: usize,
) -> Vec<Line<'a>> {
    let bordas = larguras.len() + 1;
    let ocupado: usize = larguras.iter().map(|l| l + 2).sum::<usize>() + bordas;
    if largura_total > ocupado {
        larguras[flexivel] += largura_total - ocupado;
    }
    let barra = Span::styled("│", Style::new().fg(FOSCO));
    let barra_forte = Span::styled("┃", Style::new().fg(FOSCO));

    let mut saida = vec![regua(larguras, "┏", "┳", "┓", "━")];

    // o cabecalho segue o alinhamento da coluna, como no rich: se os valores
    // vao a direita, o titulo vai junto
    let a_direita: Vec<bool> = (0..larguras.len())
        .map(|i| linhas.first().map(|l| l[i].direita).unwrap_or(false))
        .collect();

    let mut spans = vec![barra_forte.clone()];
    for (i, titulo) in cabecalho.iter().enumerate() {
        let cel = Cel::txt(
            *titulo,
            Style::new().fg(LARANJA).add_modifier(Modifier::BOLD),
            a_direita[i],
        );
        spans.push(Span::raw(" "));
        spans.extend(preenche(&cel, larguras[i]));
        spans.push(Span::raw(" "));
        spans.push(barra_forte.clone());
    }
    saida.push(Line::from(spans));
    saida.push(regua(larguras, "┡", "╇", "┩", "━"));

    for linha in linhas {
        let mut spans = vec![barra.clone()];
        for (i, cel) in linha.iter().enumerate() {
            spans.push(Span::raw(" "));
            spans.extend(preenche(cel, larguras[i]));
            spans.push(Span::raw(" "));
            spans.push(barra.clone());
        }
        saida.push(Line::from(spans));
    }
    saida.push(regua(larguras, "└", "┴", "┘", "─"));
    saida
}

// ------------------------------------------------------------ veredito ---

/// Memoria media de um tenant, medida pelos que ja rodam no cluster. E
/// estimativa, nao medida de um tenant especifico.
fn pegada_tenant(unidades: &[Arc<Mutex<Unidade>>]) -> Option<f64> {
    let prefixo = &cfg::get().prefixo_tenant;
    if prefixo.is_empty() {
        return None;
    }
    let mut mems = Vec::new();
    for u in unidades {
        let u = u.lock().unwrap();
        for c in &u.containers {
            if c.nome.starts_with(prefixo.as_str()) && c.mem > 0.0 {
                mems.push(c.mem);
            }
        }
    }
    if mems.is_empty() {
        None
    } else {
        Some(mems.iter().sum::<f64>() / mems.len() as f64)
    }
}

/// Quantos tenants ainda cabem na unidade pela memoria, respeitando o teto de
/// seguranca. So considera memoria: disco e CPU entram pelo gargalo.
fn cabem_tenants(u: &Unidade, pegada: Option<f64>) -> Option<usize> {
    let p = pegada?;
    if p <= 0.0 || u.mem_total <= 0.0 {
        return None;
    }
    let folga = u.mem_total * TETO_MEM - u.mem_usada;
    Some(if folga <= 0.0 { 0 } else { (folga / p) as usize })
}

// ------------------------------------------------------- painel por no ---

fn painel_unidade<'a>(u: &Unidade, larg_barra: usize, larg_g: usize) -> (Vec<Line<'a>>, ratatui::style::Color, String) {
    let (estado, cor_estado) = u.estado();
    let mut linhas: Vec<Line> = Vec::new();

    linhas.push(Line::from(vec![
        Span::styled(
            format!("{} ", u.rotulo()),
            Style::new().fg(AMBAR).add_modifier(Modifier::BOLD),
        ),
        fosco("・ "),
        Span::styled(
            estado,
            Style::new().fg(cor_estado).add_modifier(Modifier::BOLD),
        ),
    ]));
    linhas.push(Line::from(fosco(u.papel.clone())));
    linhas.push(Line::from(""));

    if !u.online {
        linhas.push(Line::from(Span::styled(
            format!("  SEM RESPOSTA  {}", u.erro),
            Style::new().fg(VERMELHO),
        )));
        linhas.push(Line::from(fosco(format!("  {}", u.ip))));
        return (linhas, VERMELHO, "OFFLINE".into());
    }

    for (rot, val, pct) in [
        ("CPU", u.cpu, u.cpu),
        ("MEM", u.mem_pct, u.mem_pct),
        ("DSK", u.disco_pct, u.disco_pct),
    ] {
        linhas.push(Line::from(linha_metrica(
            rot,
            &format!("{:5.1}%", val),
            pct,
            larg_barra,
        )));
    }
    linhas.push(Line::from(""));

    let hc: Vec<f64> = u.hist_cpu.iter().cloned().collect();
    let hm: Vec<f64> = u.hist_mem.iter().cloned().collect();
    let hi: Vec<f64> = u.hist_io.iter().cloned().collect();
    let pico_io = hi.iter().cloned().fold(0.0_f64, f64::max);

    for (rot, dados, cor, teto) in [
        ("cpu   ", &hc, cor_por_valor(u.cpu), Some(100.0)),
        ("mem   ", &hm, cor_por_valor(u.mem_pct), Some(100.0)),
        ("io    ", &hi, LARANJA, None),
    ] {
        let mut spans = vec![fosco(rot)];
        spans.extend(faisca(dados, larg_g, cor, teto));
        linhas.push(Line::from(spans));
    }
    // Rotulo curto de proposito: com "io pico" a linha passa de 30 colunas
    // quando o pico tem tres digitos, e quebra no painel de tres.
    linhas.push(Line::from(fosco(format!(
        "      0-100%  pico {}/s",
        bytes_h(pico_io)
    ))));
    linhas.push(Line::from(""));

    let det = |rot: &str, spans: Vec<Span<'a>>| -> Line<'a> {
        let mut v = vec![fosco(rot.to_string())];
        v.extend(spans);
        Line::from(v)
    };
    linhas.push(det(
        "  mem  ",
        vec![Span::styled(
            format!("{} / {}", bytes_h(u.mem_usada), bytes_h(u.mem_total)),
            Style::new().fg(AMBAR),
        )],
    ));
    linhas.push(det(
        "  dsk  ",
        vec![Span::styled(
            format!("{} / {}", bytes_h(u.disco_usado), bytes_h(u.disco_total)),
            Style::new().fg(AMBAR),
        )],
    ));
    linhas.push(det(
        "  net  ",
        vec![Span::styled(
            format!("↓{}/s ↑{}/s", bytes_h(u.net_rx), bytes_h(u.net_tx)),
            Style::new().fg(CIANO),
        )],
    ));
    linhas.push(det(
        "  io   ",
        vec![Span::styled(
            format!("r {}/s w {}/s", bytes_h(u.io_leitura), bytes_h(u.io_escrita)),
            Style::new().fg(LARANJA),
        )],
    ));
    linhas.push(det(
        "  load ",
        vec![
            Span::styled(
                format!("{:.2} {:.2} {:.2}", u.load.0, u.load.1, u.load.2),
                Style::new().fg(AMBAR),
            ),
            fosco(format!("  ({} cpu)", u.ncpu)),
        ],
    ));
    linhas.push(det(
        "  up   ",
        vec![
            Span::styled(dur_h(u.uptime), Style::new().fg(AMBAR)),
            fosco("   ctn "),
            Span::styled(format!("{}", u.containers.len()), Style::new().fg(AMBAR)),
        ],
    ));

    (linhas, cor_estado, u.ip.to_string())
}

// --------------------------------------------------------------- GERAL ---

pub fn aba_geral(f: &mut Frame, area: Rect, unidades: &[Arc<Mutex<Unidade>>]) {
    let partes = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(15), Constraint::Min(0)])
        .split(area);

    // --- ficha do sistema
    let vivos: Vec<usize> = (0..unidades.len())
        .filter(|i| unidades[*i].lock().unwrap().online)
        .collect();
    let online = vivos.len();
    let cpu_med = if online > 0 {
        vivos
            .iter()
            .map(|i| unidades[*i].lock().unwrap().cpu)
            .sum::<f64>()
            / online as f64
    } else {
        0.0
    };
    let ctn: usize = unidades
        .iter()
        .map(|u| u.lock().unwrap().containers.len())
        .sum();
    let tenants: usize = unidades.iter().map(|u| u.lock().unwrap().tenants()).sum();

    let mut ficha: Vec<Line> = Vec::new();
    ficha.push(Line::from(vec![
        Span::styled(
            "SISTEMA MAGI",
            Style::new().fg(LARANJA).add_modifier(Modifier::BOLD),
        ),
        fosco("   マギ システム"),
    ]));
    ficha.push(Line::from(""));
    ficha.push(Line::from(vec![
        fosco("unidades   "),
        Span::styled(
            format!("{} de {} respondendo", online, unidades.len()),
            Style::new()
                .fg(if online == unidades.len() { VERDE } else { VERMELHO })
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    ficha.push(Line::from(vec![
        fosco("carga      "),
        Span::styled(
            format!("{:.1}% em media", cpu_med),
            Style::new().fg(cor_por_valor(cpu_med)),
        ),
    ]));
    ficha.push(Line::from(vec![
        fosco("containers "),
        Span::styled(format!("{}", ctn), Style::new().fg(AMBAR)),
        fosco("   tenants "),
        Span::styled(format!("{}", tenants), Style::new().fg(AMBAR)),
    ]));

    // concentra
    let mut l = vec![fosco("concentra  ")];
    if ctn > 0 && online > 0 {
        let idx = vivos
            .iter()
            .max_by_key(|i| unidades[**i].lock().unwrap().containers.len())
            .copied()
            .unwrap();
        let u = unidades[idx].lock().unwrap();
        let n = u.containers.len();
        l.push(Span::styled(
            u.rotulo(),
            Style::new().fg(AMBAR).add_modifier(Modifier::BOLD),
        ));
        l.push(fosco(format!(
            " · {} de {} containers ({:.0}%)",
            n,
            ctn,
            n as f64 / ctn as f64 * 100.0
        )));
    } else {
        l.push(fosco("sem containers reportados"));
    }
    ficha.push(Line::from(l));

    // risco
    let mut l = vec![fosco("risco      ")];
    if vivos.is_empty() {
        l.push(Span::styled(
            "todas as unidades fora do ar",
            Style::new().fg(VERMELHO).add_modifier(Modifier::BOLD),
        ));
    } else {
        let idx = vivos
            .iter()
            .max_by(|a, b| {
                let ga = unidades[**a].lock().unwrap().gargalo().1;
                let gb = unidades[**b].lock().unwrap().gargalo().1;
                ga.partial_cmp(&gb).unwrap_or(std::cmp::Ordering::Equal)
            })
            .copied()
            .unwrap();
        let u = unidades[idx].lock().unwrap();
        let (rec, val) = u.gargalo();
        if val >= 80.0 {
            l.push(Span::styled(
                u.rotulo(),
                Style::new().fg(AMBAR).add_modifier(Modifier::BOLD),
            ));
            l.push(Span::styled(
                format!(" · {} {:.0}%", rec, val),
                Style::new().fg(LARANJA),
            ));
            l.push(fosco(if val >= 90.0 {
                ", nao comporta novo tenant"
            } else {
                ", perto do limite"
            }));
        } else {
            l.push(Span::styled(
                "nenhuma unidade perto do limite",
                Style::new().fg(VERDE),
            ));
        }
    }
    ficha.push(Line::from(l));

    // veredito
    let pegada = pegada_tenant(unidades);
    let mut l = vec![fosco("veredito   ")];
    if vivos.is_empty() {
        l.push(Span::styled(
            "SISTEMA INOPERANTE",
            Style::new().fg(VERMELHO).add_modifier(Modifier::BOLD),
        ));
    } else {
        let idx = vivos
            .iter()
            .min_by(|a, b| {
                let ga = unidades[**a].lock().unwrap().gargalo().1;
                let gb = unidades[**b].lock().unwrap().gargalo().1;
                ga.partial_cmp(&gb).unwrap_or(std::cmp::Ordering::Equal)
            })
            .copied()
            .unwrap();
        let u = unidades[idx].lock().unwrap();
        let (rec, val) = u.gargalo();
        let cabem = cabem_tenants(&u, pegada);
        if val >= 90.0 {
            l.push(Span::styled(
                "NENHUMA UNIDADE COM FOLGA",
                Style::new().fg(LARANJA).add_modifier(Modifier::BOLD),
            ));
        } else {
            l.push(Span::styled(
                u.rotulo(),
                Style::new().fg(VERDE).add_modifier(Modifier::BOLD),
            ));
            l.push(fosco(match cabem {
                None => format!(" apta · limite {} {:.0}%", rec, val),
                Some(0) => " apta, mas sem folga de memoria".to_string(),
                // a conta e por memoria; se o aperto vem de outro recurso,
                // dizer so "cabem N" enganaria
                Some(n) if rec != "mem" && val >= 70.0 => {
                    format!(" apta · ~{} pela mem, {} ja em {:.0}%", n, rec, val)
                }
                Some(n) => format!(" apta · cabem ~{} tenants pela memoria", n),
            }));
        }
    }
    ficha.push(Line::from(l));

    // --- topo: logo a esquerda, ficha a direita
    let topo_bloco = Block::default().borders(Borders::ALL).border_style(Style::new().fg(FOSCO));
    let dentro = topo_bloco.inner(partes[0]);
    f.render_widget(topo_bloco, partes[0]);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(26), Constraint::Min(0)])
        .split(dentro);
    f.render_widget(Paragraph::new(logo::emblema()), cols[0]);
    f.render_widget(Paragraph::new(ficha), cols[1]);

    // --- as tres unidades lado a lado
    let n = unidades.len().max(1) as u32;
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Ratio(1, n); unidades.len()])
        .split(partes[1]);
    for (i, u) in unidades.iter().enumerate() {
        let u = u.lock().unwrap();
        let (linhas, cor, ip) = painel_unidade(&u, 12, 24);
        let b = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(cor))
            .title(Span::styled(ip, Style::new().fg(cor)))
            .title_alignment(ratatui::layout::Alignment::Right);
        let dentro = b.inner(cols[i]);
        f.render_widget(b, cols[i]);
        f.render_widget(Paragraph::new(linhas), dentro);
    }
}

// ------------------------------------------------------------- UNIDADE ---

pub fn aba_unidade(f: &mut Frame, area: Rect, unidade: &Arc<Mutex<Unidade>>) {
    let u = unidade.lock().unwrap();
    // grafico do mesmo tamanho das barras de CPU/MEM/DSK acima
    let (linhas, cor, ip) = painel_unidade(&u, 28, 28);
    let alto = (linhas.len() + 2) as u16;

    let partes = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(alto), Constraint::Min(0)])
        .split(area);

    let b = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(cor))
        .title(Span::styled(ip, Style::new().fg(cor)))
        .title_alignment(ratatui::layout::Alignment::Right);
    let dentro = b.inner(partes[0]);
    f.render_widget(b, partes[0]);
    f.render_widget(Paragraph::new(linhas), dentro);

    let pico = u
        .containers
        .iter()
        .map(|c| c.mem)
        .fold(0.0_f64, f64::max)
        .max(1.0);
    let corpo: Vec<Vec<Cel>> = if u.containers.is_empty() {
        vec![vec![
            Cel::txt(
                if u.online {
                    "sem dados do Prometheus"
                } else {
                    "unidade offline"
                },
                Style::new().fg(AMBAR),
                false,
            ),
            Cel::txt("-", Style::new().fg(FOSCO), true),
            Cel::txt("-", Style::new().fg(FOSCO), true),
            Cel::txt("", Style::new(), false),
            Cel::txt("-", Style::new().fg(FOSCO), true),
            Cel::txt("-", Style::new().fg(FOSCO), true),
        ]]
    } else {
        u.containers
            .iter()
            .map(|c| {
                vec![
                    Cel::txt(c.nome.clone(), Style::new().fg(AMBAR), false),
                    Cel::txt(
                        format!("{:.1}", c.cpu),
                        Style::new().fg(cor_por_valor(c.cpu)),
                        true,
                    ),
                    Cel::txt(bytes_h(c.mem), Style::new().fg(AMBAR), true),
                    Cel::spans(barra(c.mem / pico * 100.0, 12), 12),
                    Cel::txt(
                        if c.rx > 0.0 { bytes_h(c.rx) } else { "-".into() },
                        Style::new().fg(CIANO),
                        true,
                    ),
                    Cel::txt(
                        if c.tx > 0.0 { bytes_h(c.tx) } else { "-".into() },
                        Style::new().fg(CIANO),
                        true,
                    ),
                ]
            })
            .collect()
    };

    let mut larguras = [16usize, 6, 9, 12, 9, 9];
    let tab = tabela(
        &["CONTAINER", "CPU%", "MEM", "USO", "RX/s", "TX/s"],
        &mut larguras,
        0,
        corpo,
        partes[1].width as usize,
    );
    f.render_widget(Paragraph::new(tab), partes[1]);
}

// ------------------------------------------------------------ DIAGRAMA ---

pub fn aba_diagrama(f: &mut Frame, area: Rect, unidades: &[Arc<Mutex<Unidade>>]) {
    // borda arredondada e respiro interno, como o Panel do rich: sem eles o
    // titulo encosta na moldura e a aba fica com outra cara
    let b = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(FOSCO))
        .padding(Padding::new(2, 2, 1, 1));
    let dentro = b.inner(area);
    f.render_widget(b, area);

    let mut linhas: Vec<Line> = vec![
        Line::from(Span::styled(
            "DIAGRAMA DE DISPONIBILIDADE",
            Style::new().fg(LARANJA).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    let largura = dentro.width as usize;
    for l in painel_unidades(unidades) {
        let comp: usize = l.spans.iter().map(|s| s.content.chars().count()).sum();
        let pad = largura.saturating_sub(comp) / 2;
        let mut spans = vec![Span::raw(" ".repeat(pad))];
        spans.extend(l.spans);
        linhas.push(Line::from(spans));
    }

    linhas.push(Line::from(""));
    linhas.push(Line::from(fosco("latencia ate cada unidade (tailnet)")));

    let corpo: Vec<Vec<Cel>> = unidades
        .iter()
        .map(|u| {
            let u = u.lock().unwrap();
            let hist: Vec<f64> = u.hist_latencia.iter().cloned().collect();
            let nome = Cel::txt(
                u.rotulo(),
                Style::new().fg(AMBAR).add_modifier(Modifier::BOLD),
                false,
            );
            match u.latencia_ms {
                None => vec![
                    nome,
                    Cel::txt("sem resposta", Style::new().fg(VERMELHO), true),
                    Cel::spans(faisca(&hist, 22, VERMELHO, None), 22),
                    Cel::txt("-", Style::new().fg(FOSCO), true),
                    Cel::txt("-", Style::new().fg(FOSCO), true),
                ],
                Some(l) => {
                    let cor = cor_latencia(l);
                    let mn = hist.iter().cloned().fold(f64::INFINITY, f64::min).min(l);
                    let mx = hist.iter().cloned().fold(0.0_f64, f64::max).max(l);
                    vec![
                        nome,
                        Cel::txt(format!("{:.0}ms", l), Style::new().fg(cor), true),
                        Cel::spans(faisca(&hist, 22, cor, None), 22),
                        Cel::txt(format!("{:.0}ms", mn), Style::new(), true),
                        Cel::txt(format!("{:.0}ms", mx), Style::new(), true),
                    ]
                }
            }
        })
        .collect();

    let mut larguras = [16usize, 8, 22, 7, 7];
    linhas.extend(tabela(
        &["UNIDADE", "PING", "HISTORICO", "MIN", "MAX"],
        &mut larguras,
        2,
        corpo,
        largura,
    ));

    f.render_widget(Paragraph::new(linhas), dentro);
}

fn cor_latencia(ms: f64) -> ratatui::style::Color {
    if ms < 50.0 {
        VERDE
    } else if ms < 150.0 {
        AMBAR
    } else {
        LARANJA
    }
}

// ------------------------------------------------------- topo e rodape ---

pub fn cabecalho(f: &mut Frame, area: Rect, aba: usize, unidades: &[Arc<Mutex<Unidade>>], hora: &str) {
    let b = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(LARANJA));
    let dentro = b.inner(area);
    f.render_widget(b, area);

    // as laterais levam o que precisam e o meio fica com o resto: por
    // proporcao, a faixa de abas nao cabia e "5 DIAGRAMA" saia cortado
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(20),
            Constraint::Min(0),
            Constraint::Length(16),
        ])
        .split(dentro);

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "MAGI",
                Style::new().fg(LARANJA).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" SYSTEM", Style::new().fg(AMBAR)),
            fosco("  マギ"),
        ])),
        cols[0],
    );

    let mut nomes: Vec<String> = vec!["GERAL".into()];
    nomes.extend(unidades.iter().map(|u| u.lock().unwrap().nome.clone()));
    nomes.push("DIAGRAMA".into());
    let mut spans = Vec::new();
    for (i, nome) in nomes.iter().enumerate() {
        let txt = format!(" {} {} ", i + 1, nome);
        spans.push(if i == aba {
            Span::styled(
                txt,
                Style::new()
                    .fg(ratatui::style::Color::Black)
                    .bg(AMBAR)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            fosco(txt)
        });
        spans.push(Span::raw(" "));
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)).alignment(ratatui::layout::Alignment::Center),
        cols[1],
    );

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(hora, Style::new().fg(AMBAR))))
            .alignment(ratatui::layout::Alignment::Right),
        cols[2],
    );
}

pub fn rodape(f: &mut Frame, area: Rect) {
    let mut spans = Vec::new();
    for (tecla, desc) in [
        ("1-5", "abas"),
        ("TAB", "ciclar"),
        ("r", "atualizar"),
        ("q", "sair"),
    ] {
        spans.push(Span::styled(
            format!(" {} ", tecla),
            Style::new()
                .fg(ratatui::style::Color::Black)
                .bg(FOSCO)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(fosco(format!(" {}   ", desc)));
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)).alignment(ratatui::layout::Alignment::Center),
        area,
    );
}

