//! Configuracao e paleta.
//!
//! A lista de nos nao fica no codigo de proposito: cada instalacao tem a sua,
//! e assim da pra versionar o programa sem versionar o inventario.

use std::path::PathBuf;
use std::sync::OnceLock;

use ratatui::style::Color;

pub const AMBAR: Color = Color::Rgb(0xff, 0xb0, 0x00);
pub const LARANJA: Color = Color::Rgb(0xff, 0x7b, 0x00);
/// So pra OFFLINE.
pub const VERMELHO: Color = Color::Rgb(0xff, 0x31, 0x31);
/// Topo da escala de uso, sem invadir o vermelho.
pub const LARANJA_FORTE: Color = Color::Rgb(0xff, 0x5f, 0x00);
pub const VERDE: Color = Color::Rgb(0x7d, 0xff, 0x7d);
pub const FOSCO: Color = Color::Rgb(0x8a, 0x5a, 0x00);
pub const CIANO: Color = Color::Rgb(0x00, 0xd7, 0xff);
/// Arcos de fundo do logo.
pub const VERDE_ARCO: Color = Color::Rgb(0x2f, 0x6b, 0x52);
/// Texto vazado dentro dos blocos do painel.
pub const PRETO: Color = Color::Rgb(0x0c, 0x0c, 0x0c);

pub const BLOCOS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

pub const COLETORES: [&str; 8] = [
    "cpu", "meminfo", "filesystem", "netdev", "diskstats", "loadavg", "time", "stat",
];

/// Acima disso a unidade e tratada como sem folga de memoria.
pub const TETO_MEM: f64 = 0.85;

#[derive(Clone)]
pub struct No {
    pub nome: String,
    pub ord: String,
    pub ip: String,
    /// Como a maquina aparece no label `instance` do Prometheus.
    pub prom: String,
    /// Como a maquina aparece no label de caixa das metricas de banco. Costuma
    /// divergir do `prom`: os dois nomes mudam em momentos diferentes, e um nao
    /// deduz o outro. Vazio significa "este no nao hospeda tenant".
    pub caixa: String,
    /// Rotulo curto do no na aba CLUSTER, onde a coluna tem tres colunas.
    pub sigla: String,
    pub papel: String,
}

/// Nomes de job e de label da instalacao. Sem este bloco a aba CLUSTER nao
/// existe: a vista por tenant depende de saber como o Prometheus local chama
/// as coisas, e isso nao da para adivinhar.
pub struct ClusterCfg {
    pub job_banco: String,
    pub job_replica: String,
    pub job_sonda: String,
    pub label_tenant: String,
    pub label_box: String,
    /// Nome do servico da aplicacao dentro da stack de cada tenant.
    pub servico_app: String,
    pub servico_banco: String,
}

pub struct Config {
    pub prometheus: String,
    pub intervalo_host: u64,
    pub intervalo_container: u64,
    pub historico: usize,
    /// Prefixo que identifica um container de tenant, pro veredito saber o
    /// que contar. Vazio desliga a contagem.
    pub prefixo_tenant: String,
    /// Tamanho em que o painel cabe sem quebrar linha: a largura vem da aba
    /// GERAL e a altura da aba DIAGRAMA, que e a mais alta.
    pub largura_ideal: u16,
    pub altura_ideal: u16,
    /// Nos que nao rodam cAdvisor: dizem o motivo em vez de tabela vazia.
    pub sem_cadvisor: Vec<String>,
    /// Janela da faixa de disponibilidade, em minutos, e quantos blocos ela
    /// tem. Largura fixa porque a faixa e janela de tempo, nao proporcao.
    pub janela_sonda: i64,
    pub blocos_sonda: usize,
    pub cluster: Option<ClusterCfg>,
    pub nos: Vec<No>,
}

impl Config {
    pub fn tem_cluster(&self) -> bool {
        self.cluster.is_some()
    }

    pub fn sem_cadvisor(&self, prom: &str) -> bool {
        self.sem_cadvisor.iter().any(|x| x == prom)
    }
}

static CONFIG: OnceLock<Config> = OnceLock::new();

fn candidatos() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(c) = std::env::var("MAGI_CONFIG") {
        v.push(PathBuf::from(c));
    }
    v.push(PathBuf::from("magi.json"));
    if let Ok(casa) = std::env::var("HOME") {
        v.push(PathBuf::from(format!("{}/.config/magi/config.json", casa)));
    }
    v
}

/// Le a configuracao do primeiro lugar que existir. Erro aqui e fatal: sem
/// inventario nao ha o que monitorar.
pub fn carregar() -> Result<(), String> {
    let caminho = candidatos()
        .into_iter()
        .find(|p| p.is_file())
        .ok_or_else(|| {
            "nao achei configuracao. Copie magi.example.json para magi.json \
             (ou aponte MAGI_CONFIG) e ajuste os nos."
                .to_string()
        })?;
    let bruto = std::fs::read_to_string(&caminho).map_err(|e| e.to_string())?;
    let j: serde_json::Value = serde_json::from_str(&bruto)
        .map_err(|e| format!("{}: {}", caminho.display(), e))?;

    let nos: Vec<No> = j["unidades"]
        .as_array()
        .ok_or("configuracao sem a lista `unidades`")?
        .iter()
        .map(|u| No {
            nome: u["nome"].as_str().unwrap_or("?").to_string(),
            ord: u["ord"].as_str().unwrap_or("").to_string(),
            ip: u["ip"].as_str().unwrap_or("").to_string(),
            prom: u["prom"].as_str().unwrap_or("").to_string(),
            caixa: u["caixa"].as_str().unwrap_or("").to_string(),
            sigla: u["sigla"].as_str().unwrap_or("").to_string(),
            papel: u["papel"].as_str().unwrap_or("").to_string(),
        })
        .collect();
    if nos.is_empty() {
        return Err("configuracao sem nenhuma unidade".into());
    }

    let texto = |chave: &str, padrao: &str| -> String {
        j[chave].as_str().unwrap_or(padrao).to_string()
    };
    let num = |chave: &str, padrao: u64| -> u64 { j[chave].as_u64().unwrap_or(padrao) };

    // job_banco e o unico obrigatorio: sem replica ou sem sonda a aba ainda
    // serve, e as colunas correspondentes ficam vazias
    let bc = &j["cluster"];
    let cluster = match bc["job_banco"].as_str() {
        Some(jb) if !jb.is_empty() => Some(ClusterCfg {
            job_banco: jb.to_string(),
            job_replica: bc["job_replica"].as_str().unwrap_or("").to_string(),
            job_sonda: bc["job_sonda"].as_str().unwrap_or("").to_string(),
            label_tenant: bc["label_tenant"].as_str().unwrap_or("tenant").to_string(),
            label_box: bc["label_box"].as_str().unwrap_or("box").to_string(),
            servico_app: bc["servico_app"].as_str().unwrap_or("").to_string(),
            servico_banco: bc["servico_banco"].as_str().unwrap_or("").to_string(),
        }),
        _ => None,
    };

    let sem_cadvisor: Vec<String> = j["sem_cadvisor"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let cfg = Config {
        prometheus: std::env::var("MAGI_PROM").unwrap_or_else(|_| texto("prometheus", "")),
        intervalo_host: num("intervalo_host", 1),
        // coletar mais rapido que o scrape do Prometheus nao cria dado novo,
        // so encurta a espera entre o scrape chegar e a tela mostrar
        intervalo_container: num("intervalo_container", 3),
        historico: num("historico", 60) as usize,
        prefixo_tenant: texto("prefixo_tenant", ""),
        largura_ideal: num("largura_ideal", 106) as u16,
        altura_ideal: num("altura_ideal", 40) as u16,
        sem_cadvisor,
        janela_sonda: num("janela_sonda", 30) as i64,
        blocos_sonda: num("blocos_sonda", 30) as usize,
        cluster,
        nos,
    };
    CONFIG.set(cfg).map_err(|_| "configuracao ja carregada".to_string())
}

pub fn get() -> &'static Config {
    CONFIG.get().expect("configuracao nao carregada")
}
