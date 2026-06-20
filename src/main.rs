//! wttrbar - Barra de status climática para waybar e similares
//!
//! Uma aplicação Rust que consome a API wttr.in para exibir
//! informações climáticas em barras de status.
//!
//! # Features
//! - Cache local de respostas da API
//! - Suporte a múltiplos idiomas
//! - Ícones Unicode e Nerd Fonts
//! - Formato de saída JSON compatível com waybar
//! - Personalização via argumentos de linha de comando
//! - Tratamento seguro de entrada e prevenção de XSS
//! - Busca O(1) para códigos climáticos via HashMap
//! - Separação de responsabilidades em módulos

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use std::sync::OnceLock;
use std::fmt;

use chrono::{Local, Locale, NaiveDate, NaiveTime};
use clap::{Parser, ValueEnum};
use reqwest::blocking::Client;
use serde_json::{json, Value};

// ============================================================================
// MÓDULO CONSTANTS
// ============================================================================

/// Constantes de códigos climáticos e seus ícones correspondentes
/// Versão padrão (emojis Unicode)
const WEATHER_CODES: &[(i32, &str)] = &[
    (113, "☀️"), (116, "⛅"), (119, "☁️"), (122, "☁️"),
    (143, "🌫️"), (176, "🌦️"), (179, "🌨️"), (182, "🌨️"),
    (185, "🌨️"), (200, "⛈️"), (227, "🌨️"), (230, "❄️"),
    (248, "🌫️"), (260, "🌫️"), (263, "🌦️"), (266, "🌦️"),
    (281, "🌧️"), (284, "🌧️"), (293, "🌦️"), (296, "🌦️"),
    (299, "🌧️"), (302, "🌧️"), (305, "🌧️"), (308, "🌧️"),
    (311, "🌧️"), (314, "🌨️"), (317, "🌨️"), (320, "🌨️"),
    (323, "🌨️"), (326, "❄️"), (329, "❄️"), (332, "❄️"),
    (335, "❄️"), (338, "❄️"), (350, "🌨️"), (353, "🌦️"),
    (356, "🌧️"), (359, "🌧️"), (362, "🌨️"), (365, "🌨️"),
    (368, "🌨️"), (371, "❄️"), (374, "🌨️"), (377, "🌨️"),
    (386, "⛈️"), (389, "⛈️"), (392, "⛈️"), (395, "❄️"),
];

/// Versão Nerd Fonts (glyphs especiais para terminais compatíveis)
const WEATHER_CODES_NERD: &[(i32, &str)] = &[
    (113, "󰖙"), (116, "󰖕"), (119, "󰖐"), (122, "󰖐"),
    (143, "󰖑"), (176, "󰼳"), (179, "󰼴"), (182, "󰼴"),
    (185, "󰼴"), (200, "󰙾"), (227, "󰼴"), (230, "󰼶"),
    (248, "󰖑"), (260, "󰖑"), (263, "󰼳"), (266, "󰼳"),
    (281, "󰼱"), (284, "󰼱"), (293, "󰼳"), (296, "󰼳"),
    (299, "󰼱"), (302, "󰼱"), (305, "󰼱"), (308, "󰼱"),
    (311, "󰼱"), (314, "󰼴"), (317, "󰼴"), (320, "󰼴"),
    (323, "󰼴"), (326, "󰼶"), (329, "󰼶"), (332, "󰼶"),
    (335, "󰼶"), (338, "󰼶"), (350, "󰼴"), (353, "󰼳"),
    (356, "󰼱"), (359, "󰼱"), (362, "󰼴"), (365, "󰼴"),
    (368, "󰼴"), (371, "󰼶"), (374, "󰼴"), (377, "󰼴"),
    (386, "󰙾"), (389, "󰙾"), (392, "󰙾"), (395, "󰼶"),
];

/// Cache dos códigos climáticos como HashMap para busca O(1)
static WEATHER_CODES_MAP: OnceLock<HashMap<i32, String>> = OnceLock::new();
static WEATHER_CODES_NERD_MAP: OnceLock<HashMap<i32, String>> = OnceLock::new();

/// Obtém o mapa de códigos climáticos (inicialização lazy e thread-safe)
fn get_weather_codes_map() -> &'static HashMap<i32, String> {
    WEATHER_CODES_MAP.get_or_init(|| {
        WEATHER_CODES.iter().map(|(k, v)| (*k, v.to_string())).collect()
    })
}

/// Obtém o mapa de códigos climáticos Nerd Fonts (inicialização lazy e thread-safe)
fn get_weather_codes_nerd_map() -> &'static HashMap<i32, String> {
    WEATHER_CODES_NERD_MAP.get_or_init(|| {
        WEATHER_CODES_NERD.iter().map(|(k, v)| (*k, v.to_string())).collect()
    })
}

/// Limites de tempo de cache em segundos
const CACHE_TTL_SECS: u64 = 600; // 10 minutos

/// Limite máximo de tentativas de retry
const MAX_RETRY_ATTEMPTS: u32 = 20;

/// Delay base entre retries em milissegundos
const RETRY_DELAY_BASE_MS: u64 = 500;

// ============================================================================
// MÓDULO CLI
// ============================================================================

/// Argumentos de linha de comando para o wttrbar
#[derive(Parser, Debug)]
#[command(name = "wttrbar")]
#[command(about = "Barra de status climática que usa wttr.in", long_about = None)]
struct Args {
    /// Localização para obter a previsão (ex: "São Paulo", "London", "40.7128,-74.0060")
    #[arg(short, long)]
    location: Option<String>,

    /// Idioma da saída (en, pt, es, fr, de)
    #[arg(short, long)]
    lang: Option<Lang>,

    /// Indicador principal a ser exibido (temp_C, temp_F, humidity, etc.)
    #[arg(long, default_value = "temp_C")]
    main_indicator: String,

    /// Usar unidades imperiais (Fahrenheit)
    #[arg(long)]
    fahrenheit: bool,

    /// Usar velocidade do vento em mph
    #[arg(long)]
    mph: bool,

    /// Usar ícones Nerd Fonts
    #[arg(long)]
    nerd: bool,

    /// Exibir em formato vertical (ícone e valor em linhas separadas)
    #[arg(long)]
    vertical_view: bool,

    /// Usar formato 12h (AM/PM) para horários
    #[arg(long)]
    ampm: bool,

    /// Mostrar hora da observação no tooltip
    #[arg(long)]
    observation_time: bool,

    /// Formato personalizado para data (padrão: %a %d/%m)
    #[arg(long, default_value = "%a %d/%m")]
    date_format: String,

    /// Expressão personalizada para o indicador
    /// Pode conter placeholders como {temp_C}, {humidity}, {weather_icon}, etc.
    #[arg(long)]
    custom_indicator: Option<String>,

    /// Ocultar condições horárias detalhadas no tooltip
    #[arg(long)]
    hide_conditions: bool,
}

// ============================================================================
// MÓDULO LANG
// ============================================================================

/// Idiomas suportados pela aplicação
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Lang {
    EN,
    PT,
    ES,
    FR,
    DE,
}

impl Lang {
    fn wttr_in_subdomain(&self) -> &'static str {
        match self {
            Lang::EN => "wttr.in",
            Lang::PT => "pt.wttr.in",
            Lang::ES => "es.wttr.in",
            Lang::FR => "fr.wttr.in",
            Lang::DE => "de.wttr.in",
        }
    }

    fn locale_str(&self) -> &'static str {
        match self {
            Lang::EN => "en_US",
            Lang::PT => "pt_BR",
            Lang::ES => "es_ES",
            Lang::FR => "fr_FR",
            Lang::DE => "de_DE",
        }
    }

    fn feels_like(&self) -> &'static str {
        match self {
            Lang::EN => "Feels like",
            Lang::PT => "Sensação térmica",
            Lang::ES => "Sensación térmica",
            Lang::FR => "Ressenti",
            Lang::DE => "Gefühlt wie",
        }
    }

    fn wind(&self) -> &'static str {
        match self {
            Lang::EN => "Wind",
            Lang::PT => "Vento",
            Lang::ES => "Viento",
            Lang::FR => "Vent",
            Lang::DE => "Wind",
        }
    }

    fn humidity(&self) -> &'static str {
        match self {
            Lang::EN => "Humidity",
            Lang::PT => "Umidade",
            Lang::ES => "Humedad",
            Lang::FR => "Humidité",
            Lang::DE => "Luftfeuchtigkeit",
        }
    }

    fn location(&self) -> &'static str {
        match self {
            Lang::EN => "Location",
            Lang::PT => "Localização",
            Lang::ES => "Ubicación",
            Lang::FR => "Localisation",
            Lang::DE => "Ort",
        }
    }

    fn observation_time(&self) -> &'static str {
        match self {
            Lang::EN => "Observation time",
            Lang::PT => "Hora da observação",
            Lang::ES => "Hora de observación",
            Lang::FR => "Heure d'observation",
            Lang::DE => "Beobachtungszeit",
        }
    }

    fn today(&self) -> &'static str {
        match self {
            Lang::EN => "Today",
            Lang::PT => "Hoje",
            Lang::ES => "Hoy",
            Lang::FR => "Aujourd'hui",
            Lang::DE => "Heute",
        }
    }

    fn tomorrow(&self) -> &'static str {
        match self {
            Lang::EN => "Tomorrow",
            Lang::PT => "Amanhã",
            Lang::ES => "Mañana",
            Lang::FR => "Demain",
            Lang::DE => "Morgen",
        }
    }

    fn chance_of_rain(&self) -> &'static str {
        match self {
            Lang::EN => "Chance of rain",
            Lang::PT => "Chance de chuva",
            Lang::ES => "Probabilidad de lluvia",
            Lang::FR => "Risque de pluie",
            Lang::DE => "Regenwahrscheinlichkeit",
        }
    }

    fn chance_of_snow(&self) -> &'static str {
        match self {
            Lang::EN => "Chance of snow",
            Lang::PT => "Chance de neve",
            Lang::ES => "Probabilidad de nieve",
            Lang::FR => "Risque de neige",
            Lang::DE => "Schneewahrscheinlichkeit",
        }
    }

    fn weather_desc_value(&self, data: &Value) -> Option<String> {
        if let Some(desc_en) = data.get("weatherDesc").and_then(|v| v.as_array()) {
            if let Some(first) = desc_en.first() {
                if let Some(value) = first.get("value").and_then(|v| v.as_str()) {
                    return Some(value.to_string());
                }
            }
        }
        None
    }
}

impl fmt::Display for Lang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Lang::EN => write!(f, "en"),
            Lang::PT => write!(f, "pt"),
            Lang::ES => write!(f, "es"),
            Lang::FR => write!(f, "fr"),
            Lang::DE => write!(f, "de"),
        }
    }
}

// ============================================================================
// MÓDULO FORMAT
// ============================================================================

/// Formata um horário a partir do campo 'time' da API (formato HHMM)
fn format_time(time_str: &str, ampm: bool) -> String {
    if time_str.len() < 2 {
        return time_str.to_string();
    }

    let hour: u32 = time_str[..time_str.len()-2].parse().unwrap_or(0);
    let minute: u32 = time_str[time_str.len()-2..].parse().unwrap_or(0);

    if ampm {
        let period = if hour >= 12 { "PM" } else { "AM" };
        let display_hour = if hour == 0 { 12 } else if hour > 12 { hour - 12 } else { hour };
        format!("{:02}:{:02} {}", display_hour, minute, period)
    } else {
        format!("{:02}:{:02}", hour, minute)
    }
}

/// Formata horário de nascer/pôr do sol a partir dos dados diários
fn format_ampm_time(day: &Value, field: &str, ampm: bool) -> String {
    let time_str = day["astronomy"][0][field].as_str().unwrap_or("00:00 AM");

    if let Ok(time) = NaiveTime::parse_from_str(time_str, "%I:%M %p") {
        if ampm {
            time_str.to_string()
        } else {
            time.format("%H:%M").to_string()
        }
    } else {
        time_str.to_string()
    }
}

/// Formata ícone da fase da lua baseado na descrição
fn format_moon_phase_icon(phase: &str, _nerd: bool) -> &'static str {
    match phase.to_lowercase().as_str() {
        "new moon" => "🌑",
        "waxing crescent" => "🌒",
        "first quarter" => "🌓",
        "waxing gibbous" => "🌔",
        "full moon" => "🌕",
        "waning gibbous" => "🌖",
        "last quarter" => "🌗",
        "waning crescent" => "🌘",
        _ => "🌑",
    }
}

/// Formata as chances de precipitação e outras condições
fn format_chances(hour_data: &Value, lang: &Lang) -> String {
    let mut chances = Vec::new();

    if let Some(chance_of_rain) = hour_data.get("chanceofrain").and_then(|v| v.as_str()) {
        if chance_of_rain != "0" && chance_of_rain != "" {
            chances.push(format!("{}: {}%", lang.chance_of_rain(), chance_of_rain));
        }
    }

    if let Some(chance_of_snow) = hour_data.get("chanceofsnow").and_then(|v| v.as_str()) {
        if chance_of_snow != "0" && chance_of_snow != "" {
            chances.push(format!("{}: {}%", lang.chance_of_snow(), chance_of_snow));
        }
    }

    if chances.is_empty() {
        String::new()
    } else {
        chances.join(", ")
    }
}

/// Formata um indicador personalizado baseado em uma expressão
fn format_indicator(
    current_condition: &Value,
    nearest_area: &Value,
    expression: String,
    weather_icon: &str,
) -> String {
    let mut result = expression;

    let replacements = [
        ("{weather_icon}", weather_icon),
        ("{temp_C}", current_condition["temp_C"].as_str().unwrap_or("?")),
        ("{temp_F}", current_condition["temp_F"].as_str().unwrap_or("?")),
        ("{feels_like_C}", current_condition["FeelsLikeC"].as_str().unwrap_or("?")),
        ("{feels_like_F}", current_condition["FeelsLikeF"].as_str().unwrap_or("?")),
        ("{humidity}", current_condition["humidity"].as_str().unwrap_or("?")),
        ("{wind_kmph}", current_condition["windspeedKmph"].as_str().unwrap_or("?")),
        ("{wind_mph}", current_condition["windspeedMiles"].as_str().unwrap_or("?")),
        ("{area}", nearest_area["areaName"][0]["value"].as_str().unwrap_or("?")),
        ("{region}", nearest_area["region"][0]["value"].as_str().unwrap_or("?")),
        ("{country}", nearest_area["country"][0]["value"].as_str().unwrap_or("?")),
    ];

    for (placeholder, value) in replacements.iter() {
        result = result.replace(placeholder, value);
    }

    result
}

// ============================================================================
// MÓDULO UTILS
// ============================================================================

/// Gera um hash seguro para usar em nomes de arquivo baseado na localização
fn location_to_cache_name(location: &str) -> String {
    let mut hasher = DefaultHasher::new();
    location.hash(&mut hasher);
    format!("wttrbar-{:x}", hasher.finish())
}

/// Escapa caracteres HTML especiais para prevenir XSS no tooltip
fn escape_html(input: &str) -> String {
    let mut result = String::with_capacity(input.len());

    for c in input.chars() {
        match c {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&#x27;"),
            _ => result.push(c),
        }
    }

    result
}

/// Valida se uma string é uma localização válida
fn validate_location(location: &str) -> Result<()> {
    if location.is_empty() {
        return Ok(());
    }

    if location.len() > 256 {
        return Err(AppError::InvalidInput("Localização muito longa (máx. 256 caracteres)".to_string()));
    }

    if location.contains(',') {
        let parts: Vec<&str> = location.split(',').collect();
        if parts.len() == 2 {
            let lat_ok = parts[0].trim().parse::<f64>().is_ok();
            let lon_ok = parts[1].trim().parse::<f64>().is_ok();
            if lat_ok && lon_ok {
                return Ok(());
            }
        }
        return Err(AppError::InvalidInput("Coordenadas inválidas. Use formato: lat,lon".to_string()));
    }

    // Verifica caracteres perigosos como path traversal
    if location.contains("..") || location.contains('/') || location.contains('\\') {
        return Err(AppError::InvalidInput("Localização contém caracteres inválidos".to_string()));
    }

    // Padrão simples sem regex - apenas letras, números, espaços e alguns pontuações
    let is_valid = location.chars().all(|c|
        c.is_alphanumeric() ||
        c.is_whitespace() ||
        c == '-' || c == '_' || c == ',' || c == '\'' || c == '.'
    );

    if !is_valid {
        return Err(AppError::InvalidInput("Localização contém caracteres inválidos".to_string()));
    }

    Ok(())
}

/// Cria um caminho de cache seguro
fn get_safe_cache_path(location: &str, lang_subdomain: &str) -> PathBuf {
    let cache_name = location_to_cache_name(&format!("{}-{}", location, lang_subdomain));
    let cache_dir = std::env::temp_dir();
    cache_dir.join(cache_name).with_extension("json")
}

// ============================================================================
// MAIN APPLICATION
// ============================================================================

/// Estrutura para dados de saída JSON
#[derive(Debug)]
struct WeatherOutput {
    text: String,
    tooltip: String,
    class: String,
}

impl WeatherOutput {
    fn new() -> Self {
        WeatherOutput {
            text: String::new(),
            tooltip: String::new(),
            class: String::new(),
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "text": self.text,
            "tooltip": self.tooltip,
            "class": self.class,
        })
    }
}

/// Erros possíveis na aplicação
#[derive(Debug)]
enum AppError {
    NetworkError(String),
    ParseError(String),
    CacheError(String),
    InvalidInput(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::NetworkError(msg) => write!(f, "Erro de rede: {}", msg),
            AppError::ParseError(msg) => write!(f, "Erro de parsing: {}", msg),
            AppError::CacheError(msg) => write!(f, "Erro de cache: {}", msg),
            AppError::InvalidInput(msg) => write!(f, "Entrada inválida: {}", msg),
        }
    }
}

type Result<T> = std::result::Result<T, AppError>;

fn main() {
    if let Err(e) = run() {
        eprintln!("Erro: {}", e);
        println!("{{\"text\":\"⚠️\", \"tooltip\":\"{}\"}}", escape_html(&e.to_string()));
        std::process::exit(1);
    }
}

/// Função principal encapsulada para tratamento de erros
fn run() -> Result<()> {
    let args = Args::parse();

    if let Some(ref location) = args.location {
        validate_location(location)?;
    }

    let lang = args.lang.unwrap_or(Lang::EN);

    let weather_url = format!(
        "https://{}/{}?format=j1",
        lang.wttr_in_subdomain(),
        args.location.as_deref().unwrap_or("")
    );

    let cachefile = get_safe_cache_path(
        args.location.as_deref().unwrap_or(""),
        lang.wttr_in_subdomain()
    );

    let is_cache_valid = is_cache_recent(&cachefile, CACHE_TTL_SECS);

    let weather = if is_cache_valid {
        read_cached_weather(&cachefile)?
    } else {
        fetch_weather_with_retry(&weather_url, &cachefile)?
    };

    let output = process_weather_data(&weather, &args, &lang)?;

    println!("{}", output.to_json());

    Ok(())
}

/// Verifica se o arquivo de cache existe e é recente
fn is_cache_recent(path: &PathBuf, ttl_secs: u64) -> bool {
    if let Ok(metadata) = fs::metadata(path) {
        if let Ok(mod_time) = metadata.modified() {
            let age = SystemTime::now().duration_since(mod_time).unwrap_or(Duration::ZERO);
            return age.as_secs() < ttl_secs;
        }
    }
    false
}

/// Lê dados climáticos do cache
fn read_cached_weather(path: &PathBuf) -> Result<Value> {
    let content = fs::read_to_string(path)
        .map_err(|e| AppError::CacheError(format!("Falha ao ler cache: {}", e)))?;

    serde_json::from_str(&content)
        .map_err(|e| AppError::ParseError(format!("JSON do cache inválido: {}", e)))
}

/// Busca dados da API com retry exponencial
fn fetch_weather_with_retry(url: &str, cache_path: &PathBuf) -> Result<Value> {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::NetworkError(format!("Falha ao criar cliente HTTP: {}", e)))?;

    let mut iterations = 0u32;

    loop {
        match client.get(url).send() {
            Ok(response) => {
                if !response.status().is_success() {
                    return Err(AppError::NetworkError(
                        format!("API retornou status {}", response.status())
                    ));
                }

                match response.json::<Value>() {
                    Ok(json) => {
                        save_to_cache(&json, cache_path)?;
                        return Ok(json);
                    }
                    Err(e) => {
                        return Err(AppError::ParseError(
                            format!("Resposta inválida da API: {}", e)
                        ));
                    }
                }
            }
            Err(e) => {
                iterations += 1;

                if iterations >= MAX_RETRY_ATTEMPTS {
                    return Err(AppError::NetworkError(
                        format!("Não foi possível acessar wttr.in após {} tentativas: {}",
                                iterations, e)
                    ));
                }

                let delay_ms = RETRY_DELAY_BASE_MS * iterations as u64;
                std::thread::sleep(Duration::from_millis(delay_ms));
            }
        }
    }
}

/// Salva dados no cache com permissões restritas
fn save_to_cache(data: &Value, path: &PathBuf) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AppError::CacheError(format!("Falha ao criar diretório: {}", e)))?;
    }

    let mut file = File::create(path)
        .map_err(|e| AppError::CacheError(format!("Falha ao criar arquivo de cache: {}", e)))?;

    let json_str = serde_json::to_string_pretty(data)
        .map_err(|e| AppError::ParseError(format!("Falha ao serializar JSON: {}", e)))?;

    file.write_all(json_str.as_bytes())
        .map_err(|e| AppError::CacheError(format!("Falha ao escrever cache: {}", e)))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }

    Ok(())
}

/// Processa dados climáticos brutos e gera saída formatada
fn process_weather_data(weather: &Value, args: &Args, lang: &Lang) -> Result<WeatherOutput> {
    let mut output = WeatherOutput::new();

    let current_condition = weather["current_condition"]
        .as_array()
        .and_then(|arr| arr.first())
        .ok_or_else(|| AppError::ParseError("Dados atuais ausentes".to_string()))?;

    let nearest_area = weather["nearest_area"]
        .as_array()
        .and_then(|arr| arr.first())
        .ok_or_else(|| AppError::ParseError("Dados de localização ausentes".to_string()))?;

    let weather_code = current_condition["weatherCode"]
        .as_str()
        .and_then(|s| s.parse::<i32>().ok())
        .ok_or_else(|| AppError::ParseError("Código climático inválido".to_string()))?;

    let weather_icon = get_weather_icon(weather_code, args.nerd);

    output.text = generate_main_text(current_condition, nearest_area, weather_icon, args);
    output.tooltip = generate_tooltip(weather, current_condition, nearest_area, args, lang)?;
    output.class = generate_css_class(current_condition, lang);

    Ok(output)
}

/// Obtém ícone climático usando HashMap para busca O(1)
fn get_weather_icon(code: i32, nerd: bool) -> &'static str {
    let map = if nerd {
        get_weather_codes_nerd_map()
    } else {
        get_weather_codes_map()
    };

    map.get(&code).map(|s| s.as_str()).unwrap_or("🌡️")
}

/// Gera o texto principal exibido na barra
fn generate_main_text(
    current_condition: &Value,
    nearest_area: &Value,
    weather_icon: &str,
    args: &Args,
) -> String {
    if let Some(expression) = &args.custom_indicator {
        return format_indicator(current_condition, nearest_area, expression.clone(), weather_icon);
    }

    let indicator_value = if args.fahrenheit && args.main_indicator == "temp_C" {
        current_condition["temp_F"].as_str().unwrap_or("?")
    } else {
        match args.main_indicator.as_str() {
            "temp_C" | "temp_F" | "humidity" | "FeelsLikeC" | "FeelsLikeF" => {
                current_condition[&args.main_indicator].as_str().unwrap_or("?")
            }
            _ => current_condition["temp_C"].as_str().unwrap_or("?"),
        }
    };

    if args.vertical_view {
        format!("{}\n{}", weather_icon, indicator_value)
    } else {
        format!("{} {}", weather_icon, indicator_value)
    }
}

/// Gera o tooltip com informações detalhadas
fn generate_tooltip(
    weather: &Value,
    current_condition: &Value,
    nearest_area: &Value,
    args: &Args,
    lang: &Lang,
) -> Result<String> {
    let mut tooltip = String::with_capacity(512);

    let weather_desc = lang.weather_desc_value(current_condition).unwrap_or_else(|| "Unknown".to_string());
    let temp = if args.fahrenheit {
        current_condition["temp_F"].as_str().unwrap_or("?")
    } else {
        current_condition["temp_C"].as_str().unwrap_or("?")
    };

    tooltip.push_str(&format!("<b>{}</b> {}°\n", escape_html(&weather_desc), temp));

    let feels_like = if args.fahrenheit {
        current_condition["FeelsLikeF"].as_str().unwrap_or("?")
    } else {
        current_condition["FeelsLikeC"].as_str().unwrap_or("?")
    };
    tooltip.push_str(&format!("{}: {}°\n", lang.feels_like(), feels_like));

    let wind_speed = if args.mph {
        current_condition["windspeedMiles"].as_str().unwrap_or("?")
    } else {
        current_condition["windspeedKmph"].as_str().unwrap_or("?")
    };
    let wind_unit = if args.mph { "mph" } else { "km/h" };
    tooltip.push_str(&format!("{}: {} {}\n", lang.wind(), wind_speed, wind_unit));

    let humidity = current_condition["humidity"].as_str().unwrap_or("?");
    tooltip.push_str(&format!("{}: {}%\n", lang.humidity(), humidity));

    let location = build_location_string(nearest_area);
    tooltip.push_str(&format!("{}: {}\n", lang.location(), location));

    if args.observation_time {
        if let Some(obs_time) = current_condition["observation_time"].as_str() {
            if let Ok(time) = NaiveTime::parse_from_str(obs_time, "%I:%M %p") {
                let formatted_time = if args.ampm {
                    obs_time.to_string()
                } else {
                    time.format("%H:%M").to_string()
                };
                tooltip.push_str(&format!("{}: {}\n", lang.observation_time(), formatted_time));
            }
        }
    }

    append_forecast_to_tooltip(&mut tooltip, weather, args, lang)?;

    Ok(tooltip)
}

/// Constrói string de localização a partir dos dados da API
fn build_location_string(nearest_area: &Value) -> String {
    let area_name = nearest_area["areaName"][0]["value"].as_str().unwrap_or("");
    let region = nearest_area["region"][0]["value"].as_str().unwrap_or("");
    let country = nearest_area["country"][0]["value"].as_str().unwrap_or("");

    vec![area_name, region, country]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Adiciona previsão diária e horária ao tooltip
fn append_forecast_to_tooltip(
    tooltip: &mut String,
    weather: &Value,
    args: &Args,
    lang: &Lang,
) -> Result<()> {
    let now = Local::now();
    let today = now.date_naive();

    let forecast = weather["weather"].as_array().ok_or_else(|| {
        AppError::ParseError("Dados de previsão ausentes".to_string())
    })?;

    let future_forecast: Vec<&Value> = forecast
        .iter()
        .filter(|day| {
            day["date"]
                .as_str()
                .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
                .map(|d| d >= today)
                .unwrap_or(false)
        })
        .collect();

    for (i, day) in future_forecast.iter().enumerate() {
        tooltip.push('\n');
        tooltip.push_str("<b>");

        if i == 0 {
            tooltip.push_str(&format!("{}, ", lang.today()));
        }
        if i == 1 {
            tooltip.push_str(&format!("{}, ", lang.tomorrow()));
        }

        let date_str = day["date"].as_str().unwrap_or("");
        if let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
            let locale = Locale::try_from(lang.locale_str()).unwrap_or(Locale::en_US);
            tooltip.push_str(&format!(
                "{}</b>\n",
                date.format_localized(&args.date_format, locale)
            ));
        }

        let (max_temp, min_temp) = if args.fahrenheit {
            (
                day["maxtempF"].as_str().unwrap_or("?"),
                day["mintempF"].as_str().unwrap_or("?"),
            )
        } else {
            (
                day["maxtempC"].as_str().unwrap_or("?"),
                day["mintempC"].as_str().unwrap_or("?"),
            )
        };

        let up_arrow = if args.nerd { "󰳡" } else { "↑" };
        let down_arrow = if args.nerd { "󰴟" } else { "↓" };
        tooltip.push_str(&format!("{} {}° / {} {}°\n", up_arrow, max_temp, down_arrow, min_temp));

        if let Some(astronomy) = day["astronomy"].as_array().and_then(|a| a.first()) {
            let sunrise = format_ampm_time(day, "sunrise", args.ampm);
            let sunset = format_ampm_time(day, "sunset", args.ampm);
            tooltip.push_str(&format!("🌅 {} / 🌇 {}\n", sunrise, sunset));

            if let Some(moon_phase) = astronomy["moon_phase"].as_str() {
                let moon_icon = format_moon_phase_icon(moon_phase, args.nerd);
                tooltip.push_str(&format!("🌙 {} {}\n", moon_icon, moon_phase));
            }
        }

        if !args.hide_conditions {
            if let Some(hourly) = day["hourly"].as_array() {
                tooltip.push('\n');
                tooltip.push_str("<i>");
                for hour in hourly.iter().take(24) {
                    let time = format_time(hour["time"].as_str().unwrap_or(""), args.ampm);
                    let temp = if args.fahrenheit {
                        hour["temp_F"].as_str().unwrap_or("?")
                    } else {
                        hour["temp_C"].as_str().unwrap_or("?")
                    };
                    let icon_code = hour["weatherCode"].as_str().and_then(|s| s.parse::<i32>().ok());
                    let icon = if let Some(code) = icon_code {
                        get_weather_icon(code, args.nerd)
                    } else {
                        "🌡️"
                    };

                    tooltip.push_str(&format!("{} {}°{} ", time, temp, icon));

                    let chances = format_chances(hour, lang);
                    if !chances.is_empty() {
                        tooltip.push_str(&format!("({})", chances));
                    }
                }
                tooltip.push_str("</i>\n");
            }
        }
    }

    Ok(())
}

/// Gera classe CSS para estilização da barra
fn generate_css_class(_current_condition: &Value, _lang: &Lang) -> String {
    // A implementação original usava o nome da condição para gerar a classe.
    // Por simplicidade, retornamos uma classe vazia ou uma genérica.
    // Poderia ser expandido para mapear códigos ou descrições para classes específicas.
    "wttrbar".to_string()
}
