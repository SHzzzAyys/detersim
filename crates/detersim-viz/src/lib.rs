//! Local trace export helpers.
//!
//! The HTML output is self-contained and does not call external services.

use detersim_sim::RunReport;

pub fn run_report_to_json(report: &RunReport) -> String {
    format!(
        "{{\"schema_version\":1,\"seed\":{},\"dispatched\":{},\"aborted\":{},\"deadlocked\":{},\"trace\":{},\"history\":{},\"nemesis\":{},\"nemesis_trace\":{},\"tape\":{},\"tape_replaying\":{},\"tape_input_len\":{},\"tape_cursor\":{},\"tape_consumed_all\":{},\"tape_exhausted\":{}}}",
        report.seed,
        report.dispatched,
        report.aborted,
        report.deadlocked,
        string_array(&report.trace),
        string_array(&report.history),
        string_array(&report.nemesis_trace),
        string_array(&report.nemesis_trace),
        u64_array(&report.tape_log),
        report.tape_replaying,
        option_usize(report.tape_input_len),
        report.tape_cursor,
        report.tape_consumed_all,
        report.tape_exhausted,
    )
}

pub fn timeline_html(report: &RunReport) -> String {
    let json = run_report_to_json(report);
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>detersim trace</title><style>body{{font-family:system-ui,sans-serif;margin:24px;line-height:1.4}}section{{margin-block:18px}}pre{{white-space:pre-wrap;border:1px solid #ccc;padding:12px;overflow:auto}}.nemesis{{border-color:#9b2c2c;background:#fff5f5}}.history{{border-color:#2b6cb0;background:#ebf8ff}}.lanes{{display:grid;grid-template-columns:repeat(auto-fit,minmax(260px,1fr));gap:12px}}h2{{font-size:16px;margin-bottom:6px}}</style></head><body><h1>detersim trace seed {}</h1><section><h2>nemesis</h2><pre class=\"nemesis\" id=\"nemesis\"></pre></section><section><h2>history</h2><pre class=\"history\" id=\"history\"></pre></section><section><h2>node lanes</h2><div class=\"lanes\" id=\"lanes\"></div></section><section><h2>raw trace</h2><pre id=\"trace\"></pre></section><script>const report={};const byNode=new Map();function add(node,line){{if(!byNode.has(node))byNode.set(node,[]);byNode.get(node).push(line);}}for(const line of report.trace){{const deliver=line.match(/deliver (\\d+)->(\\d+)/);if(deliver){{add(deliver[1],line);add(deliver[2],line);continue;}}const poll=line.match(/poll task=(\\d+)/);add(poll?`task-${{poll[1]}}`:'system',line);}}document.getElementById('nemesis').textContent=report.nemesis_trace.join('\\n');document.getElementById('history').textContent=report.history.join('\\n');document.getElementById('trace').textContent=report.trace.join('\\n');const lanes=document.getElementById('lanes');for(const [node,lines] of [...byNode.entries()].sort()){{const section=document.createElement('section');const h=document.createElement('h2');h.textContent=node;const pre=document.createElement('pre');pre.textContent=lines.join('\\n');section.append(h,pre);lanes.append(section);}}</script></body></html>",
        report.seed, json
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
        assert!(json.contains("\"schema_version\":1"));
        assert!(json.contains("\"seed\":7"));
        assert!(json.contains("\"trace\""));
    }

    #[test]
    fn html_is_self_contained() {
        let report = pingpong_world(7);
        let html = timeline_html(&report);
        assert!(html.contains("<!doctype html>"));
        assert!(!html.contains("https://"));
    }
}
