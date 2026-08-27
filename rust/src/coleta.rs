//! Coleta hibrida: node_exporter direto de cada maquina pela tailnet a cada
//! segundo, e Prometheus pra metricas de container e historico.

use std::collections::{HashMap, VecDeque};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ratatui::style::Color;

use crate::cfg::{self, No};

pub struct Amostra {
    latencia: Option<f64>,
    corpo: Result<String, String>,
}

pub struct Container {
    pub nome: String,
    /// Nome legivel: task de Swarm vira `stack/servico`.
    pub curto: String,
    pub stack: String,
    pub svc: String,
    pub swarm: bool,
    pub cpu: f64,
    pub mem: f64,
    pub rx: f64,
    pub tx: f64,
}

#[derive(Default)]
struct Anterior {
    t: f64,
    cpu_total: f64,
    cpu_idle: f64,
    rx: f64,
    tx: f64,
    lida: f64,
    escrita: f64,
}

pub struct Unidade {
    pub nome: String,
    pub ord: String,
    pub ip: String,
    pub prom: String,
    pub papel: String,

    pub online: bool,
    pub erro: String,
    pub cpu: f64,
    pub mem_pct: f64,
    pub mem_usada: f64,
    pub mem_total: f64,
    pub disco_pct: f64,
    pub disco_usado: f64,
    pub disco_total: f64,
    pub load: (f64, f64, f64),
    pub ncpu: usize,
    pub uptime: f64,
    pub net_rx: f64,
    pub net_tx: f64,
    pub io_leitura: f64,
    pub io_escrita: f64,
    /// Tempo de resposta desta maquina ate a unidade.
    pub latencia_ms: Option<f64>,
    pub hist_latencia: VecDeque<f64>,
    pub hist_cpu: VecDeque<f64>,
    pub hist_mem: VecDeque<f64>,
    pub hist_io: VecDeque<f64>,
    pub containers: Vec<Container>,
    anterior: Option<Anterior>,
}

impl Unidade {
    pub fn nova(c: &No) -> Self {
        Unidade {
            nome: c.nome.clone(),
            ord: c.ord.clone(),
            ip: c.ip.clone(),
            prom: c.prom.clone(),
            papel: c.papel.clone(),
            online: false,
            erro: String::new(),
            cpu: 0.0,
            mem_pct: 0.0,
            mem_usada: 0.0,
            mem_total: 0.0,
            disco_pct: 0.0,
            disco_usado: 0.0,
            disco_total: 0.0,
            load: (0.0, 0.0, 0.0),
            ncpu: 1,
            uptime: 0.0,
            net_rx: 0.0,
            net_tx: 0.0,
            io_leitura: 0.0,
            io_escrita: 0.0,
            latencia_ms: None,
            hist_latencia: VecDeque::new(),
            hist_cpu: VecDeque::new(),
            hist_mem: VecDeque::new(),
            hist_io: VecDeque::new(),
            containers: Vec::new(),
            anterior: None,
        }
    }

    pub fn rotulo(&self) -> String {
        format!("{}-{}", self.nome, self.ord)
    }

    /// Vermelho fica reservado pra indisponibilidade total (OFFLINE); as duas
    /// faixas de alerta abaixo disso usam a mesma escala de `cor_por_valor`,
    /// pro rotulo do estado e o numero que o causou nao aparecerem em cores
    /// diferentes.
    pub fn estado(&self) -> (&'static str, Color) {
        if !self.online {
            return ("OFFLINE", cfg::VERMELHO);
        }
        if self.cpu >= 90.0 || self.mem_pct >= 92.0 || self.disco_pct >= 90.0 {
            return ("ALERTA", cfg::LARANJA_FORTE);
        }
        if self.cpu >= 70.0 || self.mem_pct >= 80.0 || self.disco_pct >= 80.0 {
            return ("ATENCAO", cfg::LARANJA);
        }
        ("NORMAL", cfg::VERDE)
    }

    /// CPU pela media do historico, nao pela amostra do segundo: a leitura
    /// instantanea oscila demais e faria o veredito trocar de unidade a cada
    /// quadro, o que nao serve pra decidir nada.
    pub fn cpu_estavel(&self) -> f64 {
        if self.hist_cpu.is_empty() {
            self.cpu
        } else {
            self.hist_cpu.iter().sum::<f64>() / self.hist_cpu.len() as f64
        }
    }

    /// Recurso mais apertado da unidade, e quanto ele esta. E ele que limita
    /// o que ainda cabe ali, entao e por ele que a unidade e julgada.
    pub fn gargalo(&self) -> (&'static str, f64) {
        let mut pior = ("cpu", self.cpu_estavel());
        for c in [("mem", self.mem_pct), ("dsk", self.disco_pct)] {
            if c.1 > pior.1 {
                pior = c;
            }
        }
        pior
    }

    /// Um container conta como tenant por prefixo ou por servico do Swarm.
    ///
    /// No Swarm o nome ganha stack e id de task, entao o teste por prefixo
    /// deixa de casar: `loja_app.1.<id>` nao comeca com `app_`. Por isso o
    /// servico extraido da task tambem vale.
    pub fn tenants(&self) -> usize {
        let p = &cfg::get().prefixo_tenant;
        let svc = cfg::get()
            .cluster
            .as_ref()
            .map(|c| c.servico_app.clone())
            .unwrap_or_default();
        if p.is_empty() && svc.is_empty() {
            return 0;
        }
        self.containers
            .iter()
            .filter(|c| {
                (!p.is_empty() && c.nome.starts_with(p.as_str()))
                    || (!svc.is_empty() && c.svc == svc)
            })
            .count()
    }

    fn url_node(&self) -> String {
        let q: Vec<String> = cfg::COLETORES
            .iter()
            .map(|c| format!("collect[]={}", c))
            .collect();
        format!("http://{}:9100/metrics?{}", self.ip, q.join("&"))
    }

    /// A parte de rede, separada do estado: roda fora do mutex, senao segurar
    /// o lock por ate 3,5s travaria o desenho da tela.
    pub fn buscar(ip: &str, url: &str) -> Amostra {
        // latencia = handshake TCP puro ate a porta do node_exporter, medido
        // a parte da busca das metricas (que carrega ~80KB e enviesaria o
        // numero pra cima). E o "ping" real desta maquina ate a unidade.
        let t_ping = Instant::now();
        let alcancavel = checar_porta(ip, 9100, Duration::from_millis(2500));
        let latencia = if alcancavel {
            Some(t_ping.elapsed().as_secs_f64() * 1000.0)
        } else {
            None
        };
        Amostra {
            latencia,
            corpo: http_get(url, Duration::from_millis(3500)),
        }
    }

    pub fn url(&self) -> String {
        self.url_node()
    }

    /// Aplica a amostra no estado. Rapido: so parse e aritmetica.
    pub fn aplicar(&mut self, amostra: Amostra) {
        let latencia = amostra.latencia;
        let corpo = match amostra.corpo {
            Ok(c) => c,
            Err(e) => {
                self.online = false;
                self.erro = e;
                self.latencia_ms = latencia;
                return;
            }
        };

        let met = parse_metrics(&corpo);
        let agora = agora_epoch();

        let cpu_total = soma(&met, "node_cpu_seconds_total", |_| true);
        let cpu_idle = soma(&met, "node_cpu_seconds_total", |l| {
            l.get("mode").map(|s| s == "idle").unwrap_or(false)
        });
        let mut cpus: Vec<&str> = met
            .iter()
            .filter(|m| m.nome == "node_cpu_seconds_total")
            .filter_map(|m| m.labels.get("cpu").map(|s| s.as_str()))
            .collect();
        cpus.sort_unstable();
        cpus.dedup();
        let ncpu = cpus.len().max(1);

        let rx = soma(&met, "node_network_receive_bytes_total", |l| {
            !ignora_net(l.get("device").map(|s| s.as_str()).unwrap_or(""))
        });
        let tx = soma(&met, "node_network_transmit_bytes_total", |l| {
            !ignora_net(l.get("device").map(|s| s.as_str()).unwrap_or(""))
        });
        let lida = soma(&met, "node_disk_read_bytes_total", |l| {
            !ignora_disco(l.get("device").map(|s| s.as_str()).unwrap_or(""))
        });
        let escrita = soma(&met, "node_disk_written_bytes_total", |l| {
            !ignora_disco(l.get("device").map(|s| s.as_str()).unwrap_or(""))
        });

        let mem_total = primeiro(&met, "node_memory_MemTotal_bytes", |_| true).unwrap_or(0.0);
        let mem_disp = primeiro(&met, "node_memory_MemAvailable_bytes", |_| true).unwrap_or(0.0);
        let raiz = |l: &HashMap<String, String>| {
            l.get("mountpoint").map(|s| s == "/").unwrap_or(false)
        };
        let d_total = primeiro(&met, "node_filesystem_size_bytes", raiz).unwrap_or(0.0);
        let d_livre = primeiro(&met, "node_filesystem_avail_bytes", raiz).unwrap_or(0.0);
        let agora_no = primeiro(&met, "node_time_seconds", |_| true).unwrap_or(agora);
        // node_boot_time_seconds vem do coletor `stat`, nao do `time`: sem ele
        // o uptime fica zerado
        let boot = primeiro(&met, "node_boot_time_seconds", |_| true).unwrap_or(0.0);

        if let Some(ant) = &self.anterior {
            let dt = agora - ant.t;
            if dt > 0.2 {
                let d_tot = cpu_total - ant.cpu_total;
                let d_idle = cpu_idle - ant.cpu_idle;
                if d_tot > 0.0 {
                    self.cpu = ((1.0 - d_idle / d_tot) * 100.0).clamp(0.0, 100.0);
                }
                self.net_rx = ((rx - ant.rx) / dt).max(0.0);
                self.net_tx = ((tx - ant.tx) / dt).max(0.0);
                self.io_leitura = ((lida - ant.lida) / dt).max(0.0);
                self.io_escrita = ((escrita - ant.escrita) / dt).max(0.0);
                empurra(&mut self.hist_cpu, self.cpu);
                empurra(&mut self.hist_mem, self.mem_pct);
                empurra(&mut self.hist_io, self.io_leitura + self.io_escrita);
            }
        }
        self.anterior = Some(Anterior {
            t: agora,
            cpu_total,
            cpu_idle,
            rx,
            tx,
            lida,
            escrita,
        });

        self.ncpu = ncpu;
        self.mem_total = mem_total;
        self.mem_usada = mem_total - mem_disp;
        self.mem_pct = if mem_total > 0.0 {
            self.mem_usada / mem_total * 100.0
        } else {
            0.0
        };
        self.disco_total = d_total;
        self.disco_usado = d_total - d_livre;
        self.disco_pct = if d_total > 0.0 {
            self.disco_usado / d_total * 100.0
        } else {
            0.0
        };
        self.load = (
            primeiro(&met, "node_load1", |_| true).unwrap_or(0.0),
            primeiro(&met, "node_load5", |_| true).unwrap_or(0.0),
            primeiro(&met, "node_load15", |_| true).unwrap_or(0.0),
        );
        self.uptime = if boot > 0.0 { agora_no - boot } else { 0.0 };
        self.latencia_ms = latencia;
        if let Some(l) = latencia {
            empurra(&mut self.hist_latencia, l);
        }
        self.online = true;
        self.erro.clear();
    }

    /// A parte de rede dos containers, tambem fora do mutex.
    pub fn buscar_containers(prom: &str) -> Vec<Container> {
        let i = prom;
        let cpu = prom_query(&format!(
            "sum by (name) (rate(container_cpu_usage_seconds_total{{instance=\"{}\",name!=\"\"}}[2m])) * 100",
            i
        ));
        let mem = prom_query(&format!(
            "sum by (name) (container_memory_usage_bytes{{instance=\"{}\",name!=\"\"}})",
            i
        ));
        let rx = prom_query(&format!(
            "sum by (name) (rate(container_network_receive_bytes_total{{instance=\"{}\",name!=\"\"}}[2m]))",
            i
        ));
        let tx = prom_query(&format!(
            "sum by (name) (rate(container_network_transmit_bytes_total{{instance=\"{}\",name!=\"\"}}[2m]))",
            i
        ));

        let mut nomes: Vec<String> = cpu.keys().chain(mem.keys()).cloned().collect();
        nomes.sort();
        nomes.dedup();
        let mut lista: Vec<Container> = nomes
            .into_iter()
            .map(|n| {
                let (curto, stack, svc, swarm) = nome_curto(&n);
                Container {
                    cpu: *cpu.get(&n).unwrap_or(&0.0),
                    mem: *mem.get(&n).unwrap_or(&0.0),
                    rx: *rx.get(&n).unwrap_or(&0.0),
                    tx: *tx.get(&n).unwrap_or(&0.0),
                    nome: n,
                    curto,
                    stack,
                    svc,
                    swarm,
                }
            })
            .collect();
        lista.sort_by(|a, b| {
            b.cpu
                .partial_cmp(&a.cpu)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.mem.partial_cmp(&a.mem).unwrap_or(std::cmp::Ordering::Equal))
        });
        lista
    }

    pub fn aplicar_containers(&mut self, lista: Vec<Container>) {
        // lista vazia costuma ser Prometheus fora do ar, nao cluster vazio:
        // manter a ultima boa evita a tabela piscar pra vazia
        if !lista.is_empty() {
            self.containers = lista;
        }
    }

    /// Preenche o grafico com o historico do Prometheus ao abrir.
    pub fn semear_historico(&mut self) {
        let _ = &self.prom;
        let cpu = prom_range(&format!(
            "(1 - avg(rate(node_cpu_seconds_total{{instance=\"{}\",mode=\"idle\"}}[2m]))) * 100",
            self.prom
        ));
        for v in cpu.into_iter().rev().take(cfg::get().historico).rev() {
            empurra(&mut self.hist_cpu, v);
        }
        let mem = prom_range(&format!(
            "(1 - node_memory_MemAvailable_bytes{{instance=\"{}\"}} / node_memory_MemTotal_bytes{{instance=\"{}\"}}) * 100",
            self.prom, self.prom
        ));
        for v in mem.into_iter().rev().take(cfg::get().historico).rev() {
            empurra(&mut self.hist_mem, v);
        }
    }
}

fn empurra(fila: &mut VecDeque<f64>, v: f64) {
    if fila.len() >= cfg::get().historico {
        fila.pop_front();
    }
    fila.push_back(v);
}

fn ignora_net(dev: &str) -> bool {
    dev.starts_with("lo")
        || dev.starts_with("docker")
        || dev.starts_with("veth")
        || dev.starts_with("br-")
        || dev.starts_with("tailscale")
}

fn ignora_disco(dev: &str) -> bool {
    dev.starts_with("loop") || dev.starts_with("ram") || dev.starts_with("dm-") || dev.starts_with("sr")
}

pub fn agora_epoch() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Teste real de alcance: se a porta nao responde, a checagem falha de verdade.
pub fn checar_porta(host: &str, porta: u16, timeout: Duration) -> bool {
    let alvo = format!("{}:{}", host, porta);
    let enderecos: Vec<SocketAddr> = match alvo.to_socket_addrs() {
        Ok(it) => it.collect(),
        Err(_) => return false,
    };
    enderecos
        .iter()
        .any(|a| TcpStream::connect_timeout(a, timeout).is_ok())
}

pub fn http_get(url: &str, timeout: Duration) -> Result<String, String> {
    let agente = ureq::AgentBuilder::new()
        .timeout_connect(timeout)
        .timeout(timeout)
        .user_agent("magi/1.0")
        .build();
    match agente.get(url).call() {
        Ok(r) => r.into_string().map_err(|e| tipo_do_erro(&e.to_string())),
        Err(e) => Err(tipo_do_erro(&e.to_string())),
    }
}

/// A tela mostra so o tipo do erro, nao a mensagem inteira: cabe na coluna e
/// e o que importa pra saber se e rede ou recusa.
fn tipo_do_erro(msg: &str) -> String {
    let baixo = msg.to_lowercase();
    if baixo.contains("timed out") || baixo.contains("timeout") {
        "Timeout".into()
    } else if baixo.contains("refused") {
        "ConnRecusada".into()
    } else if baixo.contains("dns") || baixo.contains("resolve") {
        "DNS".into()
    } else {
        "Rede".into()
    }
}

pub struct Metrica {
    pub nome: String,
    pub labels: HashMap<String, String>,
    pub valor: f64,
}

/// Converte o formato texto do Prometheus em (nome, labels, valor).
pub fn parse_metrics(texto: &str) -> Vec<Metrica> {
    let mut saida = Vec::new();
    for ln in texto.lines() {
        let ln = ln.trim();
        if ln.is_empty() || ln.starts_with('#') {
            continue;
        }
        let (cabeca, valor) = match ln.rsplit_once(char::is_whitespace) {
            Some(p) => p,
            None => continue,
        };
        let valor: f64 = match valor.trim().parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let cabeca = cabeca.trim();
        let (nome, labels) = match cabeca.split_once('{') {
            Some((n, resto)) => {
                let resto = resto.strip_suffix('}').unwrap_or(resto);
                (n, parse_labels(resto))
            }
            None => (cabeca, HashMap::new()),
        };
        saida.push(Metrica {
            nome: nome.to_string(),
            labels,
            valor,
        });
    }
    saida
}

fn parse_labels(s: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    let bytes: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        // chave
        let ini = i;
        while i < bytes.len() && bytes[i] != '=' {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let chave: String = bytes[ini..i].iter().collect::<String>().trim().trim_start_matches(',').trim().to_string();
        i += 1; // pula '='
        if i >= bytes.len() || bytes[i] != '"' {
            break;
        }
        i += 1; // pula abre-aspas
        let vi = i;
        while i < bytes.len() && bytes[i] != '"' {
            i += 1;
        }
        let valor: String = bytes[vi..i.min(bytes.len())].iter().collect();
        if i < bytes.len() {
            i += 1; // pula fecha-aspas
        }
        if !chave.is_empty() {
            m.insert(chave, valor);
        }
        while i < bytes.len() && (bytes[i] == ',' || bytes[i].is_whitespace()) {
            i += 1;
        }
    }
    m
}

pub fn soma<F>(met: &[Metrica], nome: &str, filtro: F) -> f64
where
    F: Fn(&HashMap<String, String>) -> bool,
{
    met.iter()
        .filter(|m| m.nome == nome && filtro(&m.labels))
        .map(|m| m.valor)
        .sum()
}

pub fn primeiro<F>(met: &[Metrica], nome: &str, filtro: F) -> Option<f64>
where
    F: Fn(&HashMap<String, String>) -> bool,
{
    met.iter()
        .find(|m| m.nome == nome && filtro(&m.labels))
        .map(|m| m.valor)
}

/// Nome de task de servico no Swarm: `<stack>_<servico>.<slot>.<idtask>`.
///
/// Em servico global o lugar do slot traz o id do no, nao o numero da replica,
/// e nesse caso o nome curto ganha `*`. Container que nao e task de Swarm
/// volta intacto: numa maquina com Swarm ainda sobram containers soltos, como
/// replicas de banco e exporters. Feito a mao em vez de regex para o binario
/// nao carregar a crate de regex so por isso.
pub fn nome_curto(nome: &str) -> (String, String, String, bool) {
    let simples = || (nome.to_string(), String::new(), String::new(), false);
    let partes: Vec<&str> = nome.split('.').collect();
    if partes.len() != 3 {
        return simples();
    }
    let (cabeca, slot, tarefa) = (partes[0], partes[1], partes[2]);
    let id_valido = |s: &str| s.len() >= 20 && s.chars().all(|c| c.is_ascii_alphanumeric());
    if !id_valido(tarefa) {
        return simples();
    }
    let slot_num = !slot.is_empty() && slot.chars().all(|c| c.is_ascii_digit());
    if !slot_num && !id_valido(slot) {
        return simples();
    }
    match cabeca.split_once('_') {
        Some((stack, svc)) if !stack.is_empty() && !svc.is_empty() => (
            format!("{}/{}{}", stack, svc, if slot_num { "" } else { "*" }),
            stack.to_string(),
            svc.to_string(),
            true,
        ),
        _ => simples(),
    }
}

/// Serie com os rotulos preservados. O `prom_query` joga tudo fora menos o
/// `name`, e a aba CLUSTER precisa de `tenant` e do label de caixa.
pub struct Serie {
    pub rotulos: HashMap<String, String>,
    pub valor: f64,
}

impl Serie {
    pub fn rotulo(&self, chave: &str) -> &str {
        self.rotulos.get(chave).map(|s| s.as_str()).unwrap_or("")
    }
}

pub fn prom_series(expr: &str) -> Vec<Serie> {
    let url = format!(
        "{}/api/v1/query?query={}",
        cfg::get().prometheus.clone(),
        url_encode(expr)
    );
    let mut saida = Vec::new();
    let corpo = match http_get(&url, Duration::from_secs(6)) {
        Ok(c) => c,
        Err(_) => return saida,
    };
    let json: serde_json::Value = match serde_json::from_str(&corpo) {
        Ok(v) => v,
        Err(_) => return saida,
    };
    if let Some(res) = json["data"]["result"].as_array() {
        for s in res {
            let mut rotulos = HashMap::new();
            if let Some(m) = s["metric"].as_object() {
                for (k, v) in m {
                    if let Some(t) = v.as_str() {
                        rotulos.insert(k.clone(), t.to_string());
                    }
                }
            }
            if let Some(v) = s["value"][1].as_str().and_then(|x| x.parse::<f64>().ok()) {
                saida.push(Serie { rotulos, valor: v });
            }
        }
    }
    saida
}

fn prom_query(expr: &str) -> HashMap<String, f64> {
    let url = format!(
        "{}/api/v1/query?query={}",
        cfg::get().prometheus.clone(),
        url_encode(expr)
    );
    let mut saida = HashMap::new();
    let corpo = match http_get(&url, Duration::from_secs(6)) {
        Ok(c) => c,
        Err(_) => return saida,
    };
    let json: serde_json::Value = match serde_json::from_str(&corpo) {
        Ok(v) => v,
        Err(_) => return saida,
    };
    if let Some(res) = json["data"]["result"].as_array() {
        for s in res {
            let nome = s["metric"]["name"].as_str().unwrap_or("?").to_string();
            if let Some(v) = s["value"][1].as_str().and_then(|x| x.parse::<f64>().ok()) {
                saida.insert(nome, v);
            }
        }
    }
    saida
}

fn prom_range(expr: &str) -> Vec<f64> {
    let fim = agora_epoch() as i64;
    let ini = fim - 30 * 60;
    let url = format!(
        "{}/api/v1/query_range?query={}&start={}&end={}&step=30",
        cfg::get().prometheus.clone(),
        url_encode(expr),
        ini,
        fim
    );
    let corpo = match http_get(&url, Duration::from_secs(10)) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let json: serde_json::Value = match serde_json::from_str(&corpo) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    json["data"]["result"][0]["values"]
        .as_array()
        .map(|vs| {
            vs.iter()
                .filter_map(|p| p[1].as_str().and_then(|x| x.parse::<f64>().ok()))
                .collect()
        })
        .unwrap_or_default()
}

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[cfg(test)]
mod bench {
    use super::parse_metrics;

    /// Nao e teste de correcao: mede o parser com a carga real de uma
    /// unidade, que e o trabalho que roda tres vezes por segundo.
    #[test]
    fn tempo_do_parse() {
        // pula quando nao ha carga pra medir; o caminho vem do ambiente
        //   MAGI_BENCH_PAYLOAD=/tmp/payload.txt cargo test --release -- --nocapture
        let caminho = match std::env::var("MAGI_BENCH_PAYLOAD") {
            Ok(c) => c,
            Err(_) => return,
        };
        let txt = match std::fs::read_to_string(&caminho) {
            Ok(t) => t,
            Err(_) => return,
        };
        let n = 200;
        let t0 = std::time::Instant::now();
        let mut total = 0;
        for _ in 0..n {
            total = parse_metrics(&txt).len();
        }
        let dt = t0.elapsed().as_secs_f64() / n as f64;
        println!("parse_metrics: {:.2} ms por chamada, {} metricas", dt * 1000.0, total);
    }
}
