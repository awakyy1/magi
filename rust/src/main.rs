//! magi - monitor de cluster no terminal, estilo BTOP, com a estetica da HUD
//! do sistema MAGI de Neon Genesis Evangelion.
//!
//! Fonte de dados hibrida:
//!   - node_exporter direto de cada maquina, a cada 1s
//!   - Prometheus para metricas de container e para o historico dos graficos
//!
//! Os nos vem de um arquivo de configuracao; veja magi.example.json.
//!
//! Teclas: 1..5 ou setas trocam de aba (1..6 com a aba CLUSTER configurada),
//! TAB cicla, q sai, r forca atualizacao.

mod abas;
mod cfg;
mod cluster;
mod coleta;
mod fmt;
mod logo;
mod painel;

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::{Terminal, TerminalOptions, Viewport};

use cfg::{FOSCO, LARANJA, VERDE, VERMELHO};
use cluster::Cluster;
use coleta::Unidade;

type Unidades = Vec<Arc<Mutex<Unidade>>>;

/// Pede ao terminal pra crescer ate o tamanho em que o painel cabe.
///
/// Manda a sequencia CSI 8, a mesma que o `resize` do xterm usa. E uma
/// melhoria oportunista, nao uma garantia: varios emuladores ignoram esse
/// pedido por seguranca, e dentro de tmux ou screen ela teria que ser
/// reempacotada. So aumenta, nunca encolhe: diminuir a janela de alguem seria
/// pior do que nao fazer nada.
fn ajustar_janela() {
    if std::env::var("MAGI_SEM_RESIZE").is_ok()
        || std::env::var("TMUX").is_ok()
        || std::env::var("STY").is_ok()
    {
        return;
    }
    let (larg, alt) = crossterm::terminal::size().unwrap_or((80, 24));
    let (li, ai) = (cfg::get().largura_ideal, cfg::get().altura_ideal);
    if larg >= li && alt >= ai {
        return;
    }
    print!("\x1b[8;{};{}t", alt.max(ai), larg.max(li));
    let _ = io::stdout().flush();
    thread::sleep(Duration::from_millis(300));
}

/// Sequencia de inicializacao com checagem real de alcance de cada unidade e
/// do Prometheus, antes de entrar no dashboard.
fn tela_boot(unidades: &Unidades) {
    let (_, alt) = crossterm::terminal::size().unwrap_or((80, 24));
    print!("\x1b[2J\x1b[H");
    let _ = io::stdout().flush();

    // o logo com os arcos pede 26 linhas e a tela inteira, 36. Se o terminal
    // nao tiver essa altura, a marca curta entra no lugar pra nada rolar fora.
    let arte = if alt >= 38 {
        logo::emblema_grande()
    } else {
        logo::emblema()
    };
    let largura = crossterm::terminal::size().map(|(w, _)| w as usize).unwrap_or(80);
    for linha in &arte {
        let comp: usize = linha.spans.iter().map(|s| s.content.chars().count()).sum();
        let pad = largura.saturating_sub(comp) / 2;
        print!("{}", " ".repeat(pad));
        for s in &linha.spans {
            print!("{}", pinta(&s.content, s.style));
        }
        println!();
    }
    println!();
    centraliza(
        largura,
        &pinta_txt("INICIALIZANDO SISTEMA MAGI", LARANJA, true),
        "INICIALIZANDO SISTEMA MAGI".chars().count(),
    );
    centraliza(
        largura,
        &pinta_txt("起動シーケンス", FOSCO, false),
        "起動シーケンス".chars().count() * 2,
    );
    println!();

    for u in unidades {
        let (rotulo, ip) = {
            let u = u.lock().unwrap();
            (u.rotulo(), u.ip.clone())
        };
        let ok = coleta::checar_porta(&ip, 9100, Duration::from_millis(1500));
        println!(
            "{}{}",
            pinta_txt(&format!("  > unidade {:<22} ", rotulo), FOSCO, false),
            pinta_txt(
                if ok { "ONLINE" } else { "SEM RESPOSTA" },
                if ok { VERDE } else { VERMELHO },
                true
            )
        );
        thread::sleep(Duration::from_millis(150));
    }

    if let Some((host, ok)) = prometheus_responde(&cfg::get().prometheus) {
        println!(
            "{}{}",
            pinta_txt(
                &format!("  > prometheus {:<19} ", format!("({})", host)),
                FOSCO,
                false
            ),
            pinta_txt(
                if ok { "ONLINE" } else { "SEM RESPOSTA" },
                if ok { VERDE } else { VERMELHO },
                true
            )
        );
    }
    println!();
    centraliza(
        largura,
        &pinta_txt("carregando historico do Prometheus...", FOSCO, false),
        "carregando historico do Prometheus...".chars().count(),
    );
}

/// Teste real de alcance do Prometheus, usado na tela de boot.
fn prometheus_responde(url: &str) -> Option<(String, bool)> {
    if url.is_empty() {
        return None;
    }
    let sem_esquema = url.split("://").nth(1).unwrap_or(url);
    let autoridade = sem_esquema.split('/').next().unwrap_or(sem_esquema);
    let (host, porta) = match autoridade.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse().unwrap_or(9090)),
        None => (autoridade.to_string(), 9090u16),
    };
    let ok = coleta::checar_porta(&host, porta, Duration::from_millis(1500));
    Some((host, ok))
}

fn centraliza(largura: usize, texto_pintado: &str, comprimento: usize) {
    let pad = largura.saturating_sub(comprimento) / 2;
    println!("{}{}", " ".repeat(pad), texto_pintado);
}

fn pinta_txt(s: &str, cor: ratatui::style::Color, negrito: bool) -> String {
    let mut e = Style::new().fg(cor);
    if negrito {
        e = e.add_modifier(Modifier::BOLD);
    }
    pinta(s, e)
}

/// Escreve com cor sem depender do ratatui: a tela de boot roda antes de
/// entrar no modo alternativo, imprimindo direto no terminal.
fn pinta(s: &str, estilo: Style) -> String {
    let mut cod = String::new();
    if let Some(ratatui::style::Color::Rgb(r, g, b)) = estilo.fg {
        cod.push_str(&format!("\x1b[38;2;{};{};{}m", r, g, b));
    }
    if estilo.add_modifier.contains(Modifier::BOLD) {
        cod.push_str("\x1b[1m");
    }
    format!("{}{}\x1b[0m", cod, s)
}

fn agora_hhmmss() -> String {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    // horario local sem dependencia externa: o deslocamento vem do proprio
    // sistema, via `date`, uma vez por processo nao vale a pena — entao usa
    // UTC deslocado pelo offset lido na abertura
    let secs = d.as_secs() as i64 + *DESLOC;
    let dia = secs % 86400;
    let (h, m, s) = (dia / 3600, (dia % 3600) / 60, dia % 60);
    let dias = secs / 86400;
    let (ano, mes, diam) = civil(dias);
    let _ = ano;
    format!("{:02}/{:02} {:02}:{:02}:{:02}", diam, mes, h, m, s)
}

/// Dias desde a epoca -> (ano, mes, dia). Algoritmo de Howard Hinnant.
fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

static DESLOC: std::sync::LazyLock<i64> = std::sync::LazyLock::new(|| {
    std::process::Command::new("date")
        .arg("+%z")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| {
            let s = s.trim();
            let sinal = if s.starts_with('-') { -1 } else { 1 };
            let h: i64 = s.get(1..3)?.parse().ok()?;
            let m: i64 = s.get(3..5)?.parse().ok()?;
            Some(sinal * (h * 3600 + m * 60))
        })
        .unwrap_or(0)
});

fn main() -> io::Result<()> {
    if let Err(e) = cfg::carregar() {
        eprintln!("magi: {}", e);
        std::process::exit(1);
    }
    let unidades: Unidades = cfg::get()
        .nos
        .iter()
        .map(|c| Arc::new(Mutex::new(Unidade::nova(c))))
        .collect();

    // Antes do boot, nao depois: assim o logo e a checagem ja aparecem na
    // geometria final e nao ha um salto de tamanho ao entrar no painel.
    ajustar_janela();
    tela_boot(&unidades);

    // semeia o historico em paralelo
    let mut jobs = Vec::new();
    for u in &unidades {
        let u = Arc::clone(u);
        jobs.push(thread::spawn(move || {
            let mut g = u.lock().unwrap();
            g.semear_historico();
        }));
    }
    for j in jobs {
        let _ = j.join();
    }

    let parar = Arc::new(AtomicBool::new(false));
    let estado_cluster = Arc::new(Mutex::new(Cluster::default()));
    laco_host(&unidades, Arc::clone(&parar));
    laco_containers(&unidades, Arc::clone(&parar));
    laco_cluster(
        &unidades,
        Arc::clone(&estado_cluster),
        Arc::clone(&parar),
    );

    let r = rodar_ui(&unidades, &estado_cluster);
    parar.store(true, Ordering::Relaxed);
    r
}

/// Coleta as unidades em paralelo: uma thread por unidade.
///
/// A busca de rede roda fora do mutex e so o resultado entra sob lock, senao
/// segurar o lock durante o HTTP travaria o desenho da tela por ate 3,5s.
fn laco_host(unidades: &Unidades, parar: Arc<AtomicBool>) {
    for u in unidades {
        let u = Arc::clone(u);
        let parar = Arc::clone(&parar);
        thread::spawn(move || {
            while !parar.load(Ordering::Relaxed) {
                let (ip, url) = {
                    let g = u.lock().unwrap();
                    (g.ip.clone(), g.url())
                };
                let amostra = Unidade::buscar(&ip, &url);
                u.lock().unwrap().aplicar(amostra);
                thread::sleep(Duration::from_secs(cfg::get().intervalo_host));
            }
        });
    }
}

fn laco_containers(unidades: &Unidades, parar: Arc<AtomicBool>) {
    for u in unidades {
        let u = Arc::clone(u);
        let parar = Arc::clone(&parar);
        thread::spawn(move || {
            while !parar.load(Ordering::Relaxed) {
                let prom = { u.lock().unwrap().prom.clone() };
                let lista = Unidade::buscar_containers(&prom);
                u.lock().unwrap().aplicar_containers(lista);
                thread::sleep(Duration::from_secs(cfg::get().intervalo_container));
            }
        });
    }
}

/// A vista por tenant depende do consumo que o laco de containers traz, entao
/// espera um ciclo antes da primeira coleta: comecar junto mostraria a tabela
/// com as colunas de CPU e memoria vazias.
fn laco_cluster(unidades: &Unidades, estado: Arc<Mutex<Cluster>>, parar: Arc<AtomicBool>) {
    if !cfg::get().tem_cluster() {
        return;
    }
    let unidades: Unidades = unidades.to_vec();
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(2));
        while !parar.load(Ordering::Relaxed) {
            let r = cluster::buscar(&unidades);
            estado.lock().unwrap().aplicar(r);
            thread::sleep(Duration::from_secs(cfg::get().intervalo_container));
        }
    });
}

fn rodar_ui(unidades: &Unidades, estado_cluster: &Arc<Mutex<Cluster>>) -> io::Result<()> {
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(io::stdout());
    let mut term = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Fullscreen,
        },
    )?;

    // GERAL + uma por unidade + DIAGRAMA, e CLUSTER so quando configurada
    let com_cluster = cfg::get().tem_cluster();
    let total_abas = unidades.len() + 2 + usize::from(com_cluster);
    let mut aba = 0usize;
    let mut ultimo = Instant::now();

    let saida = loop {
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(k) = event::read()? {
                if k.kind == KeyEventKind::Press {
                    match k.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') => break Ok(()),
                        KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                            break Ok(())
                        }
                        KeyCode::Char(c) if c.is_ascii_digit() => {
                            let i = c.to_digit(10).unwrap() as usize;
                            if i >= 1 && i <= total_abas {
                                aba = i - 1;
                            }
                        }
                        KeyCode::Tab | KeyCode::Right => aba = (aba + 1) % total_abas,
                        KeyCode::Left => aba = (aba + total_abas - 1) % total_abas,
                        KeyCode::Char('r') | KeyCode::Char('R') => {
                            for u in unidades {
                                let u = Arc::clone(u);
                                thread::spawn(move || {
                                    let prom = { u.lock().unwrap().prom.clone() };
                                    let lista = Unidade::buscar_containers(&prom);
                                    u.lock().unwrap().aplicar_containers(lista);
                                });
                            }
                            if com_cluster {
                                let us: Unidades = unidades.to_vec();
                                let est = Arc::clone(estado_cluster);
                                thread::spawn(move || {
                                    let r = cluster::buscar(&us);
                                    est.lock().unwrap().aplicar(r);
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        if ultimo.elapsed() >= Duration::from_millis(250) {
            ultimo = Instant::now();
            let hora = agora_hhmmss();
            term.draw(|f| {
                let partes = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3),
                        Constraint::Min(0),
                        Constraint::Length(1),
                    ])
                    .split(f.area());
                abas::cabecalho(f, partes[0], aba, unidades, &hora);
                if aba == 0 {
                    abas::aba_geral(f, partes[1], unidades);
                } else if com_cluster && aba == total_abas - 1 {
                    abas::aba_cluster(f, partes[1], unidades, estado_cluster);
                } else if aba == unidades.len() + 1 {
                    abas::aba_diagrama(f, partes[1], unidades);
                } else {
                    abas::aba_unidade(f, partes[1], &unidades[aba - 1]);
                }
                abas::rodape(f, partes[2]);
            })?;
        }
    };

    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;
    println!("{}", pinta_txt("MAGI SYSTEM encerrado.", FOSCO, false));
    saida
}

