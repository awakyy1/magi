//! Visao por tenant do cluster.
//!
//! Monitoramento por no responde "a maquina esta saudavel?". Quando a maquina
//! hospeda muitos tenants independentes, essa e a unidade errada: um no em 15%
//! de CPU pode ter um tenant fora do ar. Aqui o mesmo dado vira uma linha por
//! tenant.
//!
//! Nao usa metrica `swarm_*`: o dockerd so expoe isso se a instalacao ligar
//! `metrics-addr`, o que exige reiniciar o daemon. Entao o no de cada tenant
//! sai do label de caixa das metricas de banco, e o consumo sai do nome das
//! tasks no cAdvisor.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::cfg;
use crate::coleta::{self, Unidade};

#[derive(Clone, Default)]
pub struct Linha {
    pub tenant: String,
    pub no: String,
    pub no_rep: String,
    pub db_up: bool,
    pub site: Option<f64>,
    pub latencia: Option<f64>,
    pub app_cpu: Option<f64>,
    pub app_mem: Option<f64>,
    pub db_cpu: Option<f64>,
    pub db_mem: Option<f64>,
    pub tem_task: bool,
    /// Disponibilidade recente, um valor por bloco da faixa, mais novo no fim.
    pub hist: Vec<f64>,
    pub rep_existe: bool,
    pub rep_ok: bool,
    pub rep_lag: Option<f64>,
}

impl Linha {
    pub fn cpu_total(&self) -> f64 {
        self.app_cpu.unwrap_or(0.0) + self.db_cpu.unwrap_or(0.0)
    }
}

#[derive(Default)]
pub struct Cluster {
    pub tenants: Vec<Linha>,
    pub erro: String,
    /// Epoch em segundos da ultima coleta boa; None enquanto nao houver uma.
    pub atualizado: Option<f64>,
}

pub struct Resumo {
    pub total: usize,
    pub no_ar: usize,
    pub replicando: usize,
    pub lag_max: Option<f64>,
    pub sem_replica: Vec<String>,
    pub fora: Vec<String>,
}

impl Cluster {
    pub fn resumo(&self) -> Resumo {
        Resumo {
            total: self.tenants.len(),
            no_ar: self.tenants.iter().filter(|x| x.site == Some(1.0)).count(),
            replicando: self
                .tenants
                .iter()
                .filter(|x| x.rep_existe && x.rep_ok)
                .count(),
            lag_max: self
                .tenants
                .iter()
                .filter_map(|x| x.rep_lag)
                .fold(None, |a: Option<f64>, v| Some(a.map_or(v, |x| x.max(v)))),
            sem_replica: self
                .tenants
                .iter()
                .filter(|x| !x.rep_existe)
                .map(|x| x.tenant.clone())
                .collect(),
            fora: self
                .tenants
                .iter()
                .filter(|x| x.site == Some(0.0))
                .map(|x| x.tenant.clone())
                .collect(),
        }
    }

    pub fn aplicar(&mut self, r: Result<Vec<Linha>, String>) {
        match r {
            Ok(linhas) => {
                self.tenants = linhas;
                self.erro.clear();
                self.atualizado = Some(coleta::agora_epoch());
            }
            Err(e) => self.erro = e,
        }
    }

}

/// Faixa de disponibilidade por tenant: uma consulta de intervalo para todos,
/// em vez de uma por tenant. Cada bloco vale janela/blocos de tempo real, e a
/// faixa anda para a direita conforme o tempo passa.
fn historico_sonda(job: &str) -> HashMap<String, Vec<f64>> {
    let janela = cfg::get().janela_sonda;
    let blocos = cfg::get().blocos_sonda;
    let passo = (janela * 60 / blocos.max(1) as i64).max(15);
    let mut saida = HashMap::new();
    for (rotulos, valores) in coleta::prom_range_series(
        &format!("probe_success{{job=\"{}\"}}", job),
        janela,
        passo,
    ) {
        let tenant = rotulos.get("instance").cloned().unwrap_or_default();
        if tenant.is_empty() {
            continue;
        }
        let ini = valores.len().saturating_sub(blocos);
        saida.insert(tenant, valores[ini..].to_vec());
    }
    saida
}

fn sigla_da_caixa(caixa: &str) -> String {
    if caixa.is_empty() {
        return "?".into();
    }
    for n in &cfg::get().nos {
        if !n.caixa.is_empty() && n.caixa == caixa {
            return if n.sigla.is_empty() {
                n.nome.chars().take(3).collect()
            } else {
                n.sigla.clone()
            };
        }
    }
    "?".into()
}

/// Faz a rede e le os containers ja coletados. Roda fora do mutex do Cluster:
/// segurar o lock durante seis consultas travaria o desenho da tela.
pub fn buscar(unidades: &[Arc<Mutex<Unidade>>]) -> Result<Vec<Linha>, String> {
    let c = match &cfg::get().cluster {
        Some(c) => c,
        None => return Ok(Vec::new()),
    };

    let prim = coleta::prom_series(&format!("mysql_up{{job=\"{}\"}}", c.job_banco));
    if prim.is_empty() {
        return Err("sem resposta do Prometheus".into());
    }

    let (mut rep_io, mut rep_sql, mut rep_lag) = (Vec::new(), Vec::new(), Vec::new());
    if !c.job_replica.is_empty() {
        rep_io = coleta::prom_series(&format!(
            "mysql_slave_status_slave_io_running{{job=\"{}\"}}",
            c.job_replica
        ));
        rep_sql = coleta::prom_series(&format!(
            "mysql_slave_status_slave_sql_running{{job=\"{}\"}}",
            c.job_replica
        ));
        rep_lag = coleta::prom_series(&format!(
            "mysql_slave_status_seconds_behind_master{{job=\"{}\"}}",
            c.job_replica
        ));
    }
    let (mut sonda, mut atraso) = (Vec::new(), Vec::new());
    let mut hist: HashMap<String, Vec<f64>> = HashMap::new();
    if !c.job_sonda.is_empty() {
        sonda = coleta::prom_series(&format!("probe_success{{job=\"{}\"}}", c.job_sonda));
        atraso = coleta::prom_series(&format!(
            "probe_duration_seconds{{job=\"{}\"}}",
            c.job_sonda
        ));
        hist = historico_sonda(&c.job_sonda);
    }

    let por = |s: &[coleta::Serie], chave: &str| -> HashMap<String, f64> {
        s.iter()
            .map(|x| (x.rotulo(chave).to_string(), x.valor))
            .collect()
    };
    let caixa_de = |s: &[coleta::Serie]| -> HashMap<String, String> {
        s.iter()
            .map(|x| {
                (
                    x.rotulo(&c.label_tenant).to_string(),
                    x.rotulo(&c.label_box).to_string(),
                )
            })
            .collect()
    };

    let v_prim = por(&prim, &c.label_tenant);
    let v_io = por(&rep_io, &c.label_tenant);
    let v_sql = por(&rep_sql, &c.label_tenant);
    let v_lag = por(&rep_lag, &c.label_tenant);
    // na sonda o tenant costuma vir no proprio instance, que e o alvo
    let v_sonda = por(&sonda, "instance");
    let v_atraso = por(&atraso, "instance");
    let caixa_prim = caixa_de(&prim);
    let caixa_rep = caixa_de(&rep_io);

    // consumo vem do que o laco de containers ja trouxe: sem consulta nova
    let mut uso: HashMap<(String, String), (f64, f64)> = HashMap::new();
    for u in unidades {
        let g = u.lock().unwrap();
        for ct in &g.containers {
            if ct.stack.is_empty() {
                continue;
            }
            if ct.svc == c.servico_app || ct.svc == c.servico_banco {
                uso.insert((ct.stack.clone(), ct.svc.clone()), (ct.cpu, ct.mem));
            }
        }
    }

    let mut tenants: Vec<String> = v_prim.keys().cloned().filter(|t| !t.is_empty()).collect();
    tenants.sort();

    let mut linhas = Vec::with_capacity(tenants.len());
    for tenant in tenants {
        let app = if c.servico_app.is_empty() {
            None
        } else {
            uso.get(&(tenant.clone(), c.servico_app.clone())).copied()
        };
        let db = if c.servico_banco.is_empty() {
            None
        } else {
            uso.get(&(tenant.clone(), c.servico_banco.clone())).copied()
        };
        let tem_rep = v_io.contains_key(&tenant);
        linhas.push(Linha {
            no: sigla_da_caixa(caixa_prim.get(&tenant).map(|s| s.as_str()).unwrap_or("")),
            no_rep: if tem_rep {
                sigla_da_caixa(caixa_rep.get(&tenant).map(|s| s.as_str()).unwrap_or(""))
            } else {
                "-".into()
            },
            db_up: v_prim.get(&tenant).copied().unwrap_or(0.0) >= 1.0,
            site: v_sonda.get(&tenant).copied(),
            latencia: v_atraso.get(&tenant).copied(),
            app_cpu: app.map(|x| x.0),
            app_mem: app.map(|x| x.1),
            db_cpu: db.map(|x| x.0),
            db_mem: db.map(|x| x.1),
            tem_task: app.is_some(),
            hist: hist.get(&tenant).cloned().unwrap_or_default(),
            rep_existe: tem_rep,
            rep_ok: v_io.get(&tenant).copied().unwrap_or(0.0) >= 1.0
                && v_sql.get(&tenant).copied().unwrap_or(0.0) >= 1.0,
            rep_lag: v_lag.get(&tenant).copied(),
            tenant,
        });
    }
    Ok(linhas)
}
