//! Local trace export helpers.
//!
//! The HTML output is self-contained and does not call external services.

use detersim_sim::RunReport;

#[derive(Clone, Debug)]
pub struct DebugArtifact {
    pub title: String,
    pub run: RunReport,
    pub experiment_json: Option<String>,
    pub checker_json: Option<String>,
    pub shrink_json: Option<String>,
    pub failure_signature_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct DebugArtifactV3 {
    pub title: String,
    pub run: RunReport,
    pub experiment_json: Option<String>,
    pub search_json: Option<String>,
    pub checker_json: Option<String>,
    pub shrink_json: Option<String>,
    pub failure_signature_json: Option<String>,
    pub coverage_json: Option<String>,
    pub causal_graph_json: Option<String>,
    pub environment_json: Option<String>,
}

pub fn run_report_to_json(report: &RunReport) -> String {
    format!(
        "{{\"schema_version\":2,\"seed\":{},\"dispatched\":{},\"aborted\":{},\"deadlocked\":{},\"trace\":{},\"history\":{},\"coverage_signals\":{},\"nemesis\":{},\"nemesis_trace\":{},\"tape\":{},\"tape_events\":{},\"tape_replaying\":{},\"tape_input_len\":{},\"tape_cursor\":{},\"tape_consumed_all\":{},\"tape_exhausted\":{}}}",
        report.seed,
        report.dispatched,
        report.aborted,
        report.deadlocked,
        string_array(&report.trace),
        string_array(&report.history),
        string_array(&report.coverage_signals),
        string_array(&report.nemesis_trace),
        string_array(&report.nemesis_trace),
        u64_array(&report.tape_log),
        tape_events_json(report),
        report.tape_replaying,
        option_usize(report.tape_input_len),
        report.tape_cursor,
        report.tape_consumed_all,
        report.tape_exhausted,
    )
}

pub fn debug_artifact_v3_to_json(artifact: &DebugArtifactV3) -> String {
    format!(
        "{{\"schema_version\":3,\"title\":\"{}\",\"run\":{},\"experiment\":{},\"search\":{},\"checker\":{},\"shrink\":{},\"failure_signature\":{},\"coverage\":{},\"causal_graph\":{},\"environment\":{}}}",
        escape_json(&artifact.title),
        run_report_to_json(&artifact.run),
        option_raw_json(artifact.experiment_json.as_deref()),
        option_raw_json(artifact.search_json.as_deref()),
        option_raw_json(artifact.checker_json.as_deref()),
        option_raw_json(artifact.shrink_json.as_deref()),
        option_raw_json(artifact.failure_signature_json.as_deref()),
        option_raw_json(artifact.coverage_json.as_deref()),
        option_raw_json(artifact.causal_graph_json.as_deref()),
        option_raw_json(artifact.environment_json.as_deref()),
    )
}

pub fn debug_artifact_schema_version(json: &str) -> Option<u32> {
    let marker = "\"schema_version\":";
    let start = json.find(marker)? + marker.len();
    let digits: String = json[start..]
        .chars()
        .skip_while(|ch| ch.is_ascii_whitespace())
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

pub fn debug_artifact_to_json(artifact: &DebugArtifact) -> String {
    format!(
        "{{\"schema_version\":2,\"title\":\"{}\",\"run\":{},\"experiment\":{},\"checker\":{},\"shrink\":{},\"failure_signature\":{}}}",
        escape_json(&artifact.title),
        run_report_to_json(&artifact.run),
        option_raw_json(artifact.experiment_json.as_deref()),
        option_raw_json(artifact.checker_json.as_deref()),
        option_raw_json(artifact.shrink_json.as_deref()),
        option_raw_json(artifact.failure_signature_json.as_deref()),
    )
}

pub fn timeline_html(report: &RunReport) -> String {
    debug_artifact_html(&DebugArtifact {
        title: format!("detersim trace seed {}", report.seed),
        run: report.clone(),
        experiment_json: None,
        checker_json: None,
        shrink_json: None,
        failure_signature_json: None,
    })
}

pub fn debug_artifact_html(artifact: &DebugArtifact) -> String {
    let json = debug_artifact_to_json(artifact);
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>detersim debug artifact</title><style>body{{font-family:system-ui,sans-serif;margin:24px;line-height:1.4}}section{{margin-block:18px}}pre{{white-space:pre-wrap;border:1px solid #ccc;padding:12px;overflow:auto}}.nemesis{{border-color:#9b2c2c;background:#fff5f5}}.history{{border-color:#2b6cb0;background:#ebf8ff}}.conflict{{border-color:#744210;background:#fffff0}}.lanes{{display:grid;grid-template-columns:repeat(auto-fit,minmax(260px,1fr));gap:12px}}h2{{font-size:16px;margin-bottom:6px}}</style></head><body><h1>{}</h1><section><h2>failure signature</h2><pre class=\"conflict\" id=\"signature\"></pre></section><section><h2>experiment</h2><pre id=\"experiment\"></pre></section><section><h2>checker / shrink</h2><pre id=\"checker\"></pre><pre id=\"shrink\"></pre></section><section><h2>nemesis</h2><pre class=\"nemesis\" id=\"nemesis\"></pre></section><section><h2>history</h2><pre class=\"history\" id=\"history\"></pre></section><section><h2>tape events</h2><pre id=\"tape\"></pre></section><section><h2>node lanes</h2><div class=\"lanes\" id=\"lanes\"></div></section><section><h2>raw trace</h2><pre id=\"trace\"></pre></section><script>const artifact={};const report=artifact.run;const byNode=new Map();function add(node,line){{if(!byNode.has(node))byNode.set(node,[]);byNode.get(node).push(line);}}for(const line of report.trace){{const deliver=line.match(/deliver (\\d+)->(\\d+)/);if(deliver){{add(deliver[1],line);add(deliver[2],line);continue;}}const poll=line.match(/poll task=(\\d+)/);add(poll?`task-${{poll[1]}}`:'system',line);}}function pretty(value){{return value===null||value===undefined?'':JSON.stringify(value,null,2);}}document.getElementById('signature').textContent=pretty(artifact.failure_signature);document.getElementById('experiment').textContent=pretty(artifact.experiment);document.getElementById('checker').textContent=pretty(artifact.checker);document.getElementById('shrink').textContent=pretty(artifact.shrink);document.getElementById('nemesis').textContent=report.nemesis_trace.join('\\n');document.getElementById('history').textContent=report.history.join('\\n');document.getElementById('tape').textContent=pretty(report.tape_events);document.getElementById('trace').textContent=report.trace.join('\\n');const lanes=document.getElementById('lanes');for(const [node,lines] of [...byNode.entries()].sort()){{const section=document.createElement('section');const h=document.createElement('h2');h.textContent=node;const pre=document.createElement('pre');pre.textContent=lines.join('\\n');section.append(h,pre);lanes.append(section);}}</script></body></html>",
        escape_json(&artifact.title),
        json
    )
}

pub fn debug_artifact_v3_html(artifact: &DebugArtifactV3) -> String {
    let json = debug_artifact_v3_to_json(artifact);
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>detersim debug artifact v3</title><style>body{{font-family:system-ui,sans-serif;margin:24px;line-height:1.4}}section{{margin-block:18px}}pre{{white-space:pre-wrap;border:1px solid #ccc;padding:12px;overflow:auto}}.nemesis{{border-color:#9b2c2c;background:#fff5f5}}.history{{border-color:#2b6cb0;background:#ebf8ff}}.conflict{{border-color:#744210;background:#fffff0}}.coverage{{border-color:#276749;background:#f0fff4}}.lanes{{display:grid;grid-template-columns:repeat(auto-fit,minmax(260px,1fr));gap:12px}}h2{{font-size:16px;margin-bottom:6px}}</style></head><body><h1>{}</h1><section><h2>run overview</h2><pre id=\"overview\"></pre></section><section><h2>failure signature</h2><pre class=\"conflict\" id=\"signature\"></pre></section><section><h2>experiment and search</h2><pre id=\"experiment\"></pre><pre id=\"search\"></pre></section><section><h2>checker witness</h2><pre class=\"conflict\" id=\"checker\"></pre></section><section><h2>shrink diff</h2><pre id=\"shrink\"></pre></section><section><h2>coverage</h2><pre class=\"coverage\" id=\"coverage\"></pre></section><section><h2>causal graph</h2><pre id=\"causal\"></pre></section><section><h2>nemesis and storage faults</h2><pre class=\"nemesis\" id=\"nemesis\"></pre></section><section><h2>client history</h2><pre class=\"history\" id=\"history\"></pre></section><section><h2>node lanes</h2><div class=\"lanes\" id=\"lanes\"></div></section><section><h2>raw trace</h2><pre id=\"trace\"></pre></section><script>const artifact={};const report=artifact.run;function pretty(value){{return value===null||value===undefined?'':JSON.stringify(value,null,2);}}document.getElementById('overview').textContent=pretty({{seed:report.seed,dispatched:report.dispatched,deadlocked:report.deadlocked,aborted:report.aborted,tape_cursor:report.tape_cursor,tape_consumed_all:report.tape_consumed_all,tape_exhausted:report.tape_exhausted}});document.getElementById('signature').textContent=pretty(artifact.failure_signature);document.getElementById('experiment').textContent=pretty(artifact.experiment);document.getElementById('search').textContent=pretty(artifact.search);document.getElementById('checker').textContent=pretty(artifact.checker);document.getElementById('shrink').textContent=pretty(artifact.shrink);document.getElementById('coverage').textContent=pretty(artifact.coverage ?? report.coverage_signals);document.getElementById('causal').textContent=pretty(artifact.causal_graph);document.getElementById('nemesis').textContent=report.nemesis_trace.join('\\n');document.getElementById('history').textContent=report.history.join('\\n');document.getElementById('trace').textContent=report.trace.join('\\n');const byNode=new Map();function add(node,line){{if(!byNode.has(node))byNode.set(node,[]);byNode.get(node).push(line);}}for(const line of report.trace){{const deliver=line.match(/deliver (\\d+)->(\\d+)/);if(deliver){{add(deliver[1],line);add(deliver[2],line);continue;}}const poll=line.match(/poll task=(\\d+)/);add(poll?`task-${{poll[1]}}`:'system',line);}}const lanes=document.getElementById('lanes');for(const [node,lines] of [...byNode.entries()].sort()){{const section=document.createElement('section');const h=document.createElement('h2');h.textContent=node;const pre=document.createElement('pre');pre.textContent=lines.join('\\n');section.append(h,pre);lanes.append(section);}}</script></body></html>",
        escape_json(&artifact.title),
        json
    )
}

fn string_array(values: &[String]) -> String {
    let items: Vec<String> = values
        .iter()
        .map(|value| format!("\"{}\"", escape_json(value)))
        .collect();
    format!("[{}]", items.join(","))
}

fn u64_array(values: &[u64]) -> String {
    let items: Vec<String> = values.iter().map(u64::to_string).collect();
    format!("[{}]", items.join(","))
}

fn tape_events_json(report: &RunReport) -> String {
    let items: Vec<String> = report
        .tape_events
        .iter()
        .map(|event| {
            format!(
                "{{\"index\":{},\"label\":\"{}\",\"value\":{}}}",
                event.index,
                event.label.as_str(),
                event.value
            )
        })
        .collect();
    format!("[{}]", items.join(","))
}

fn option_raw_json(value: Option<&str>) -> String {
    value.unwrap_or("null").to_string()
}

fn option_usize(value: Option<usize>) -> String {
    value
        .map(|n| n.to_string())
        .unwrap_or_else(|| "null".to_string())
}

fn escape_json(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use detersim_sim::scenarios::pingpong_world;

    #[test]
    fn json_contains_trace_and_seed() {
        let report = pingpong_world(7);
        let json = run_report_to_json(&report);
        assert!(json.contains("\"schema_version\":2"));
        assert!(json.contains("\"seed\":7"));
        assert!(json.contains("\"tape_events\""));
        assert!(json.contains("\"coverage_signals\""));
        assert!(json.contains("\"trace\""));
    }

    #[test]
    fn html_is_self_contained() {
        let report = pingpong_world(7);
        let html = timeline_html(&report);
        assert!(html.contains("<!doctype html>"));
        assert!(!html.contains("https://"));
    }

    #[test]
    fn debug_artifact_contains_experiment_sections() {
        let report = pingpong_world(7);
        let artifact = DebugArtifact {
            title: "debug".to_string(),
            run: report,
            experiment_json: Some("{\"case\":\"x\"}".to_string()),
            checker_json: Some("{\"explored_states\":1}".to_string()),
            shrink_json: Some("{\"ratio\":0.5}".to_string()),
            failure_signature_json: Some("{\"type\":\"InvariantViolated\"}".to_string()),
        };
        let json = debug_artifact_to_json(&artifact);
        assert!(json.contains("\"schema_version\":2"));
        assert!(json.contains("\"experiment\""));
        let html = debug_artifact_html(&artifact);
        assert!(html.contains("failure signature"));
        assert!(!html.contains("https://"));
    }

    #[test]
    fn v3_artifact_contains_search_and_coverage_sections() {
        let report = pingpong_world(7);
        let artifact = DebugArtifactV3 {
            title: "debug v3".to_string(),
            run: report,
            experiment_json: Some("{\"case\":\"x\"}".to_string()),
            search_json: Some("{\"strategy\":\"CoverageGuided\"}".to_string()),
            checker_json: Some("{\"explored_states\":1}".to_string()),
            shrink_json: Some("{\"ratio\":0.5}".to_string()),
            failure_signature_json: Some("{\"type\":\"InvariantViolated\"}".to_string()),
            coverage_json: Some("[\"message-edge:0->1\"]".to_string()),
            causal_graph_json: Some("{\"nodes\":[],\"edges\":[]}".to_string()),
            environment_json: Some("{\"binary\":\"test\"}".to_string()),
        };
        let json = debug_artifact_v3_to_json(&artifact);
        assert_eq!(debug_artifact_schema_version(&json), Some(3));
        assert!(json.contains("\"search\""));
        assert!(json.contains("\"coverage\""));
        let html = debug_artifact_v3_html(&artifact);
        assert!(html.contains("checker witness"));
        assert!(!html.contains("https://"));
    }
}
