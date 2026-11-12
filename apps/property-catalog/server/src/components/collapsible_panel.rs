use crate::auth::html_escape;

pub fn render(
    title: &str,
    subtitle: &str,
    class: &str,
    initially_open: bool,
    content: &str,
) -> String {
    let open = if initially_open { " open" } else { "" };
    format!("<details class='collapsible-panel {class}'{open}><summary><span class='collapsible-panel-summary'><strong>{}</strong><small>{}</small></span><span class='collapsible-panel-chevron' aria-hidden='true'><svg viewBox='0 0 20 20' fill='none' stroke='currentColor' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'><path d='m5 7.5 5 5 5-5'/></svg></span></summary><div class='collapsible-panel-content'>{content}</div></details>", html_escape(title), html_escape(subtitle))
}
