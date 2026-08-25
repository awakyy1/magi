//! O simbolo do MAGI, tirado da arte de referencia.
//!
//! Braille (U+2800) carrega 2x4 subpixels por celula, contra 1x2 do
//! meio-bloco e 2x2 do quadrante. E a celula certa pra line-art. O que vai
//! na tela e o CONTORNO, nao o preenchimento: preencher as faixas pra elas
//! sobreviverem a reducao transformava a marca num bloco solido, o oposto
//! da referencia.
//!
//! Os arcos concentricos vao como camada de tras, em verde apagado. Na arte
//! eles cobrem so a metade de cima; o gerador (ferramentas/gerar_logo.py)
//! mede o centro e os raios e fecha as circunferencias.

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::cfg::{LARANJA, VERDE_ARCO};

pub const LOGO_GRANDE: &[&str] = &[
    r#""#,
    r#""#,
    r#""#,
    r#""#,
    r#"             ⠰⣶⠶⠶⠶⣶⣶⡶⠶⠶⣆"#,
    r#"              ⠙⣦⣤⣾⠟⠋ ⢀⣤⣿⣦"#,
    r#"               ⠘⢯⠁⢀⣠⣾⡿⠋⠁⢘⣧"#,
    r#"                ⠈⢷⡿⠟⠁ ⣠⣴⡿⠟⢷⡀"#,
    r#"                 ⠈⢷⣀⣤⣾⠟⠋   ⢳⡀"#,
    r#"                   ⠹⡏⠁    ⢀⣤⣿⣄"#,
    r#"                    ⢹   ⣠⡶⠋⠁⢹⠹⣆"#,
    r#"                    ⣸⢀⡴⠟⠁   ⢸⡄⠉⠉⠉⠉⣿⡉⠉⢹⣿⠉⠉⣿⡏⠉⣩⠏"#,
    r#"                   ⣼⣟⡋      ⠈⡇    ⢿⡇ ⠘⣿  ⢸⣧⣰⠏"#,
    r#"                 ⢀⡼⠁⠈⠙⠓⠦⣤⣀   ⢷    ⢸⣇  ⣿⡆ ⢸⣽⠃"#,
    r#"                ⢀⡾⠁      ⠈⠙⠓⠦⣼    ⢸⣿  ⢻⡇ ⡼⠁"#,
    r#"               ⢠⡞⠿⣶⣦⣤⣀  ⢠⡞⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠁"#,
    r#"              ⢠⣯⣀⡀ ⠈⠉⠛⠿⣷⠟"#,
    r#"             ⠰⡏⠉⠛⠻⢷⣶⣤⣄⣰⠏"#,
    r#"              ⠹⣦⣄⣀  ⠉⣿⠋"#,
    r#"               ⠙⣿⠛⠿⣶⡾⠃"#,
    r#"                ⠘⢧⢀⡾⠁"#,
    r#"                 ⠈⠟"#,
    r#""#,
    r#""#,
    r#""#,
    r#""#,
];

pub const ARCOS_GRANDE: &[&str] = &[
    r#"                 ⢀⣀⡤⣤⣖⡲⠶⠶⠶⠶⠶⠶⢶⣒⣤⡤⣄⣀"#,
    r#"             ⣀⣤⣲⠿⠓⠋⠉ ⣀⣀⣀⣠⣤⣤⣄⣀⣀⣀ ⠉⠉⠚⠯⣗⡦⣀"#,
    r#"          ⣠⣴⡿⠛⠉⣀⣠⣴⠶⠛⠚⢉⣉⣉⣤⣤⣤⣤⣉⣉⣉⠛⠛⠶⢦⣄⣀⠉⠛⠿⣶⣄"#,
    r#"        ⡠⣾⠟⠁⣀⣴⠾⠋⣁⣤⡶⠟⠛⢉⢉⣉⣀⣀⣀⣀⣉⣉⡉⠛⠿⢶⣤⣈⠙⠷⣦⣀⠈⠻⢽⣦⡀"#,
    r#"      ⣠⣮⠊ ⣠⡾⠋⣡⡴⠛⠉⣁⡤⠖⠛⠋⠉⢉⣁⣤⣤⣈⣉⣉⠙⠛⠶⢦⣌⡉⠻⢶⣌⠙⢷⣄ ⠙⢿⢦"#,
    r#"    ⢀⣾⠟⠁⣠⡾⢋⣴⡾⢋⣠⡴⠊⠁⣀⠤⠖⠚⠉⠉⢉⣉⣉⣉⣉⣉⠛⠳⢶⣤⣉⠛⢷⣄⡙⠷⣦⡙⢷⡄ ⠳⣷⡄"#,
    r#"   ⢠⣿⠋⢀⣼⠏⣠⡾⢃⣴⠟⠉⣠⠴⠋⢁⣠⠴⠒⠋⠉⠉⣈⣉⣉⡉⠙⠛⠷⣦⣌⠙⠷⣦⡈⠻⣦⡈⢷⣄⠙⣆ ⠘⢟⣆"#,
    r#"  ⢠⣻⠃⢠⡾⠃⣼⠏⣠⡾⠁⣠⠞⢁⣴⠖⠉⣀⡤⠖⠋⠉⣉⣀⣀⣉⣉⠙⠓⢶⣤⡉⠻⣦⡈⠻⣦⡈⢿⣄⠹⣧⠈⢧ ⠈⢿⣆"#,
    r#" ⣤⣿⠃⢀⣾⠁⣼⠃⣰⡟⢠⡴⠃⣴⠟⢁⣴⠚⢁⣠⠔⠚⠉⢁⣀⣀⣈⠙⠛⠷⣦⣈⠻⣦⡈⠻⣦⠘⢷⡄⢻⣆⠘⣧ ⢧ ⠈⣟⡄"#,
    r#"⢠⣿⡏ ⣼⠃⣼⠇⢰⡏⢠⡿⢁⣾⠃⣴⠟⢡⡶⠋⢀⡴⠚⠉⠁  ⠈⠉⠻⢶⣌⠙⢷⡌⠻⣦⠘⣷⡈⢻⡄⢹⣆⠹⣇⠈⡇ ⠸⣽"#,
    r#"⣼⣿ ⢠⡟ ⡏⢀⡿ ⣾⠁⣾⠃⣼⠏⣰⡟⢡⡾⠋          ⠙⢷⡌⢻⡆⠹⣇⠘⣷⠈⣿ ⢿⡀⢿⡆⢸⡀ ⢯⡇"#,
    r#"⢻⡏ ⢸⠇⢸⠁⢸⡇⢸⡇⢸⡏⢠⡟⢠⡿⢀⡿⠁             ⢹⡀⠹⡄⢸⡀⠸⡄⠸⡄ ⡇⠰⣇⠸⣇ ⢸⢧"#,
    r#"⢸⡇ ⣾⢀⢸⢀⢸⢀⣼⢃⢸⢃⢸⣇⢸⢇⢸⡇               ⡇ ⡇ ⡇ ⡇ ⡇ ⣷ ⣿ ⣿ ⢸⢸"#,
    r#"⢸⡇ ⢻ ⢸ ⢸⠘⢹⡜⢸⡜⢸⡟⢸⡞⢸⡁               ⡇ ⡇ ⡇ ⡇ ⡇ ⡿ ⣿ ⣿ ⢸⢸"#,
    r#"⣼⣇ ⠘⡆⢸ ⠸⡄⠘⡇⢸⡇⠘⣇⠘⢇ ⢧              ⣰⠁⢰⠃⢰⠃⢰⠃⢰⡇⢘⡇⢠⡏⢰⡏ ⢸⡞"#,
    r#"⢻⣿  ⢇ ⡇ ⢧ ⢹⡀⠹⡀⠹⡄⠘⣆⠈⢧⡀          ⢀⡴⠃⣠⠇⢀⠏⢀⡞ ⣾ ⢸⠁⣼⠇⣸⠇ ⣞⡇"#,
    r#"⠘⣿⣇ ⠸⡄⠸⡄⠘⡆ ⢧ ⠹⡄⠙⢄⠈⠣⣀⠉⠢⣄⣀    ⢀⣠⠴⠋⢀⡴⠁⣠⠏⢀⡞ ⣼⠃⢠⠇⢰⡏⢠⡿ ⢠⣻"#,
    r#" ⠛⡿⡄ ⢳⡀⠱⡀⠘⣆⠈⢳⡀⠙⢦⠈⠳⣄⠈⠓⠦⣄⡈⠉⠉⠉⠉⢁⣀⠤⠞⠁⣠⠞⠁⣠⠏⢀⡼⠃⢰⠎⢠⡟ ⡾⠁⢀⣾⠇"#,
    r#"  ⠹⡽⡄ ⠳⡀⠱⡄⠘⢦⡀⠙⢦⡈⠑⢦⡈⠙⠲⢤⣀⣉⠉⠉⠉⠉⢉⣀⡤⠔⠋⢁⡤⠞⠁⣠⠎ ⡰⠋⢀⡞⢀⡜⠁⢀⣞⡞"#,
    r#"   ⠘⣿⡄ ⠙⣄⠘⢦⡀⠙⢄⡀⠙⠦⣀⠈⠓⠢⢄⣀⡈⠉⠉⠉⠉⢉⣀⣠⠴⠚⠉⣀⡴⠊⠁⣠⠞⠁⡰⠋⢠⠞ ⢀⣾⠏"#,
    r#"    ⠘⢿⣦⡀⠈⠣⣄⠙⢦⡀⠙⠦⣄⠈⠙⠲⠤⣄⣀⡉⠉⠉⠉⠉⢉⣀⣀⠤⠖⠚⠁⣀⡴⠊⢁⡠⠞⢁⡴⠋ ⣠⡻⠋"#,
    r#"      ⠳⣷⣄ ⠈⠢⣄⠙⠲⣄⡀⠙⠲⠤⣄⣀ ⠉⠉⠉⠉⠉⠉ ⣀⣀⠤⠖⠊⠁⣠⠴⠋⣀⡴⠋ ⣠⣾⠟⠁"#,
    r#"       ⠈⠻⣗⢤⡀⠈⠙⠦⣄⠉⠓⠦⢤⣀⡀⠉⠉⠉⠉⠉⠉⠉⠉⢁⣀⡤⠴⠚⠉⣀⠴⠚⠁ ⣠⣾⠟⠁"#,
    r#"          ⠙⠿⣶⢄⡀ ⠉⠓⠦⠤⣀⣉⡉⠉⠙⠒⠒⠚⠉⠉⣉⣀⡤⠴⠒⠋⠁⢀⣠⣴⡿⠊⠁"#,
    r#"            ⠈⠙⠻⢷⣦⢤⣀⡀  ⠉⠉⠉⠉⠉⠉⠉⠉⠁  ⣀⣠⣴⣺⠽⠋⠁"#,
    r#"                 ⠉⠓⠛⠯⠷⠶⣖⣒⣒⣒⣒⣲⠶⠾⠽⠛⠚⠉⠁"#,
];

pub const LOGO_PEQUENO: &[&str] = &[
    r#" ⣤⠤⠤⢤⣤⠤⢤"#,
    r#" ⠈⢧⡾⠋⠁⣠⡾⢳⡀"#,
    r#"  ⠈⢧⡴⠟⠁⣀⡴⢷⡀"#,
    r#"   ⠈⢳⣤⠾⠋  ⠹⡄"#,
    r#"     ⢳⡀ ⢀⡤⠞⡿⣄"#,
    r#"     ⣰⣣⠴⠋  ⢳⠈⠉⠉⣿⠉⢹⡍⠉⣏⠉⡽"#,
    r#"    ⣰⠛⠧⣄⡀  ⢸   ⢻ ⠸⡇ ⣿⡞⠁"#,
    r#"   ⣰⠃   ⠉⠙⣲⣼⣄⣀⣀⣸⣀⣀⣧⣀⡞"#,
    r#"  ⡼⠙⠓⠶⣤⣀⣰⠋⠁"#,
    r#" ⣼⠛⠲⢤⣄⡀⣸⠃"#,
    r#" ⠘⣦⣤⣀⠈⡽⠁"#,
    r#"  ⠈⢧⢉⡟⠁"#,
    r#"   ⠈⠛"#,
];

/// Compoe as duas camadas: a marca por cima, o arco onde ela nao pinta.
pub fn logo<'a>(marca: &[&str], arcos: &[&str]) -> Vec<Line<'a>> {
    let altura = marca.len().max(arcos.len());
    let mut saida = Vec::with_capacity(altura);
    for i in 0..altura {
        let m: Vec<char> = marca.get(i).map(|s| s.chars().collect()).unwrap_or_default();
        let a: Vec<char> = arcos.get(i).map(|s| s.chars().collect()).unwrap_or_default();
        let largura = m.len().max(a.len());
        let mut spans: Vec<Span> = Vec::new();
        for j in 0..largura {
            let cm = m.get(j).copied().unwrap_or(' ');
            let ca = a.get(j).copied().unwrap_or(' ');
            if cm != ' ' {
                spans.push(Span::styled(cm.to_string(), Style::new().fg(LARANJA)));
            } else if ca != ' ' {
                spans.push(Span::styled(ca.to_string(), Style::new().fg(VERDE_ARCO)));
            } else {
                spans.push(Span::raw(" "));
            }
        }
        saida.push(Line::from(spans));
    }
    saida
}

/// O simbolo com os arcos, pra tela de boot.
pub fn emblema_grande<'a>() -> Vec<Line<'a>> {
    logo(LOGO_GRANDE, ARCOS_GRANDE)
}

/// So a marca, reduzida, pro canto da aba GERAL: nesse tamanho os arcos
/// viram sujeira em vez de atmosfera.
pub fn emblema<'a>() -> Vec<Line<'a>> {
    logo(LOGO_PEQUENO, &[])
}
