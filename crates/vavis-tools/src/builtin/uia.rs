//! Controls by name, through Windows UI Automation.
//!
//! Computer use used to be screenshots and pixel guesses: take a picture,
//! have the model estimate where "Save" is, click there, take another
//! picture to see whether it worked. Slow (an image per step), expensive
//! (images are the largest thing a request can carry) and unreliable (a
//! few pixels off is a different button).
//!
//! Windows already knows where every control is. UI Automation is the
//! accessibility interface screen readers use: it lists a window's buttons,
//! fields, menu items and links with their names and positions, and can
//! press a button or fill a field directly, without moving the mouse at
//! all. These tools put that in the model's hands:
//!
//! - `list_ui_elements` -- what can be clicked or typed into, by name;
//! - `click_element` -- press one by name (Invoke/Toggle/Select, falling
//!   back to a real click at its centre);
//! - `set_element_text` -- fill a field by name, all at once;
//! - `scroll` and `drag` -- the two pointer actions that had no tool.
//!
//! Screenshots stay for what has no accessible name: images, canvases,
//! games.
//!
//! Everything reaches UI Automation through PowerShell and the .NET client
//! that ships with Windows, like the rest of this crate's Windows glue: no
//! COM bindings to compile, nothing extra to ship.

use crate::tool::{arg_num, arg_str, Domain, Param, Risk, Tool, ToolOutcome};
use serde_json::Value;

/// Most elements listed. A browser window can expose thousands; the model
/// needs the ones it can act on, and a list this long is already plenty.
const MAX_LISTED: usize = 120;

/// Longest text `set_element_text` will write in one go.
const MAX_SET_CHARS: usize = 4_000;

/// One control, as listed.
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub kind: String,
    pub name: String,
    pub automation_id: String,
    pub x: i32,
    pub y: i32,
    pub enabled: bool,
}

/// Parses the script's `kind|name|id|x|y|enabled` lines. Fields are
/// separated by a character control names never contain in practice; a
/// name that does contain one is cut there rather than misparsed.
pub fn parse_elements(output: &str) -> Vec<Element> {
    output
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\u{1f}').collect();
            if parts.len() != 6 {
                return None;
            }
            Some(Element {
                kind: parts[0].trim().to_string(),
                name: parts[1].trim().to_string(),
                automation_id: parts[2].trim().to_string(),
                x: parts[3].trim().parse().ok()?,
                y: parts[4].trim().parse().ok()?,
                enabled: parts[5].trim().eq_ignore_ascii_case("true"),
            })
        })
        .collect()
}

/// The list as the model reads it: one line per control, grouped by kind
/// is not needed -- position order is what matches the screen.
pub fn format_elements(window: &str, elements: &[Element]) -> String {
    if elements.is_empty() {
        return format!(
            "'{window}' penceresinde erişilebilir kontrol bulunamadı. \
             Uygulama UI Automation desteklemiyor olabilir; take_screenshot ile bak."
        );
    }
    let mut out = format!("Pencere: {window}\n");
    for e in elements.iter().take(MAX_LISTED) {
        let name = if e.name.is_empty() {
            format!("(adsız, id={})", e.automation_id)
        } else {
            format!("\"{}\"", e.name)
        };
        let disabled = if e.enabled { "" } else { " [devre dışı]" };
        out.push_str(&format!(
            "- {} {name} @({},{}){disabled}\n",
            e.kind, e.x, e.y
        ));
    }
    if elements.len() > MAX_LISTED {
        out.push_str(&format!(
            "… ve {} kontrol daha. Daraltmak için 'filter' kullan.\n",
            elements.len() - MAX_LISTED
        ));
    }
    out
}

/// A PowerShell single-quoted literal. Inside single quotes PowerShell
/// expands nothing -- no variables, no subexpressions -- and the only
/// escape is a doubled quote (of any of the five kinds it accepts, see
/// [`vavis_core::process::ps_quote`]). Anything the model or a window
/// supplies goes through here and nowhere else.
pub fn ps_literal(text: &str) -> String {
    vavis_core::process::ps_quote(text)
}

/// The prelude every script shares: UTF-8 output (Turkish control names
/// came back as mojibake in the console's code page), the UIA assemblies,
/// and a way to find the window the user means.
///
/// "The window the user means" is not the foreground one: when the user
/// talks to Vavis, Vavis *is* the foreground window. So without a title,
/// this takes the topmost visible, titled window that does not belong to
/// this process -- the one the user was looking at before turning to ask.
fn prelude(window: &str) -> String {
    format!(
        r#"$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class VavisWin {{
    [DllImport("user32.dll")] public static extern IntPtr GetTopWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr h, uint cmd);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
    [DllImport("user32.dll")] public static extern int GetWindowTextLength(IntPtr h);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    public static IntPtr Behind(uint self) {{
        IntPtr h = GetTopWindow(IntPtr.Zero);
        while (h != IntPtr.Zero) {{
            uint pid;
            GetWindowThreadProcessId(h, out pid);
            if (pid != self && IsWindowVisible(h) && !IsIconic(h) && GetWindowTextLength(h) > 0) return h;
            h = GetWindow(h, 2);
        }}
        return IntPtr.Zero;
    }}
}}
'@
$A = [System.Windows.Automation.AutomationElement]
$wanted = {window}
$win = $null
if ($wanted -ne '') {{
    $cond = New-Object System.Windows.Automation.PropertyCondition($A::ControlTypeProperty, [System.Windows.Automation.ControlType]::Window)
    foreach ($w in $A::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children, $cond)) {{
        if ($w.Current.Name -like ('*' + [WildcardPattern]::Escape($wanted) + '*')) {{ $win = $w; break }}
    }}
    if ($win -eq $null) {{ throw ('window not found: ' + $wanted) }}
}} else {{
    $h = [VavisWin]::Behind([uint32]{pid})
    if ($h -eq [IntPtr]::Zero) {{ throw 'no window to work in' }}
    $win = $A::FromHandle($h)
}}
$kinds = @('Button','Edit','MenuItem','ListItem','TabItem','Hyperlink','CheckBox','RadioButton','ComboBox','TreeItem','DataItem','SplitButton','MenuBar','Menu','Document','Slider','Spinner')
function Visible($e) {{
    $r = $e.Current.BoundingRectangle
    return -not $e.Current.IsOffscreen -and $r.Width -gt 0 -and $r.Height -gt 0
}}
function Center($e) {{
    $r = $e.Current.BoundingRectangle
    return @([int]($r.X + $r.Width / 2), [int]($r.Y + $r.Height / 2))
}}
"#,
        window = ps_literal(window),
        pid = std::process::id(),
    )
}

/// Lists a window's controls, one `\x1f`-separated line each.
pub fn list_script(window: &str, filter: &str) -> String {
    format!(
        r#"{prelude}
$filter = {filter}
# Escaped: a name with brackets in it is a wildcard class to -like, and
# "Page [1]" would never match itself ("Save [" would throw).
$like = '*' + [WildcardPattern]::Escape($filter) + '*'
'#WINDOW ' + $win.Current.Name
$n = 0
foreach ($e in $win.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)) {{
    $kind = $e.Current.ControlType.ProgrammaticName -replace '^ControlType\.', ''
    if ($kinds -notcontains $kind) {{ continue }}
    if (-not (Visible $e)) {{ continue }}
    $name = ($e.Current.Name -replace '[\x00-\x1f]', ' ').Trim()
    if ($filter -ne '' -and $name -notlike $like -and $kind -ne $filter) {{ continue }}
    $c = Center $e
    $kind + [char]31 + $name + [char]31 + $e.Current.AutomationId + [char]31 + $c[0] + [char]31 + $c[1] + [char]31 + $e.Current.IsEnabled
    $n++
    if ($n -ge 400) {{ break }}
}}
"#,
        prelude = prelude(window),
        filter = ps_literal(filter),
    )
}

/// Finds a control by name and acts on it. `action` is `invoke` or
/// `settext`. Prints `done|<how>` when a pattern did the job, or
/// `point|x|y` when the caller should click there itself.
pub fn act_script(
    window: &str,
    name: &str,
    kind: &str,
    nth: usize,
    action: &str,
    text: &str,
) -> String {
    format!(
        r#"{prelude}
$name = {name}
$kind = {kind}
$action = {action}
$text = {text}
$like = '*' + [WildcardPattern]::Escape($name) + '*'
$exact = @(); $partial = @()
foreach ($e in $win.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)) {{
    $k = $e.Current.ControlType.ProgrammaticName -replace '^ControlType\.', ''
    if ($kind -ne '' -and $k -ne $kind) {{ continue }}
    if (-not (Visible $e)) {{ continue }}
    $n = $e.Current.Name.Trim()
    if ($n -eq $name -or $e.Current.AutomationId -eq $name) {{ $exact += $e }}
    elseif ($n -like $like) {{ $partial += $e }}
}}
$found = @($exact + $partial)
if ($found.Count -eq 0) {{ throw ('no control named: ' + $name) }}
$i = [Math]::Min({nth}, $found.Count - 1)
$el = $found[$i]
$label = ($el.Current.ControlType.ProgrammaticName -replace '^ControlType\.', '') + ' "' + $el.Current.Name + '"'
if (-not $el.Current.IsEnabled) {{ throw ($label + ' is disabled') }}
$p = $null
if ($action -eq 'settext') {{
    if ($el.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$p)) {{
        if ($p.Current.IsReadOnly) {{ throw ($label + ' is read-only') }}
        $el.SetFocus()
        $p.SetValue($text)
        'done' + [char]31 + 'value' + [char]31 + $label
    }} else {{
        $c = Center $el
        'point' + [char]31 + $c[0] + [char]31 + $c[1] + [char]31 + $label
    }}
}} elseif ($el.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$p)) {{
    $p.Invoke(); 'done' + [char]31 + 'invoke' + [char]31 + $label
}} elseif ($el.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern, [ref]$p)) {{
    $p.Toggle(); 'done' + [char]31 + 'toggle' + [char]31 + $label
}} elseif ($el.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$p)) {{
    $p.Select(); 'done' + [char]31 + 'select' + [char]31 + $label
}} elseif ($el.TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern, [ref]$p)) {{
    $p.Expand(); 'done' + [char]31 + 'expand' + [char]31 + $label
}} else {{
    $c = Center $el
    'point' + [char]31 + $c[0] + [char]31 + $c[1] + [char]31 + $label
}}
"#,
        prelude = prelude(window),
        name = ps_literal(name),
        kind = ps_literal(kind),
        action = ps_literal(action),
        text = ps_literal(text),
        nth = nth,
    )
}

/// What an act script reported.
#[derive(Debug, Clone, PartialEq)]
pub enum Acted {
    /// A UIA pattern did it; carries how, and the control's label.
    Done { how: String, label: String },
    /// No pattern applies; click here.
    Point { x: i32, y: i32, label: String },
}

pub fn parse_acted(output: &str) -> Option<Acted> {
    let line = output.lines().rev().find(|l| l.contains('\u{1f}'))?;
    let parts: Vec<&str> = line.split('\u{1f}').collect();
    match parts.as_slice() {
        ["done", how, label] => Some(Acted::Done {
            how: how.to_string(),
            label: label.to_string(),
        }),
        ["point", x, y, label] => Some(Acted::Point {
            x: x.trim().parse().ok()?,
            y: y.trim().parse().ok()?,
            label: label.to_string(),
        }),
        _ => None,
    }
}

/// The first line of a PowerShell error, which is where it says what went
/// wrong; the rest is a stack of script positions.
fn first_line(err: &str) -> String {
    err.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("unknown error")
        .to_string()
}

#[cfg(windows)]
fn run(script: &str) -> Result<String, String> {
    super::system::run_powershell(script).map_err(|e| first_line(&e.to_string()))
}

#[cfg(not(windows))]
fn run(_script: &str) -> Result<String, String> {
    Err("UI Automation yalnızca Windows'ta var".into())
}

// ── Tools ───────────────────────────────────────────────────────────────────

pub struct ListElements;

impl Tool for ListElements {
    fn name(&self) -> &'static str {
        "list_ui_elements"
    }

    fn description(&self) -> &'static str {
        "Lists the buttons, fields, menus, tabs and links of a window by name, with \
         their positions. Faster and more reliable than a screenshot for anything \
         with a label — use it first, and click_element / set_element_text to act. \
         Without 'window' it reads the window the user was using before Vavis."
    }

    fn domain(&self) -> Domain {
        Domain::Vision
    }

    fn params(&self) -> Vec<Param> {
        vec![
            Param::optional(
                "window",
                "Part of the window title (default: the window behind Vavis)",
            ),
            Param::optional(
                "filter",
                "Only controls whose name contains this, or of this type (e.g. Button)",
            ),
        ]
    }

    fn keywords(&self) -> &'static [&'static str] {
        &[
            "düğme", "buton", "button", "pencere", "menü", "kontrol", "alan", "sekme",
        ]
    }

    fn run(&self, args: &Value) -> ToolOutcome {
        let window = arg_str(args, "window").unwrap_or("");
        let filter = arg_str(args, "filter").unwrap_or("");
        match run(&list_script(window, filter)) {
            Ok(out) => {
                let title = out
                    .lines()
                    .find_map(|l| l.strip_prefix("#WINDOW "))
                    .unwrap_or(window)
                    .to_string();
                let listed = format_elements(&title, &parse_elements(&out));
                // Control names come from other programs -- a web page's
                // button can say anything. Framed as data like any page.
                ToolOutcome::ok(crate::untrusted::wrap("ui-automation", &listed).text)
            }
            Err(e) => ToolOutcome::err(format!("kontroller okunamadı: {e}")),
        }
    }
}

pub struct ClickElement;

impl Tool for ClickElement {
    fn name(&self) -> &'static str {
        "click_element"
    }

    fn description(&self) -> &'static str {
        "Presses a control by its name (from list_ui_elements): a button, menu item, \
         tab, checkbox, link. Uses the control's own action where it has one, so it \
         works even if the window is partly covered."
    }

    fn domain(&self) -> Domain {
        Domain::Vision
    }

    fn risk(&self) -> Risk {
        Risk::Destructive
    }

    fn params(&self) -> Vec<Param> {
        vec![
            Param::required("name", "The control's name, or its automation id"),
            Param::optional(
                "type",
                "Control type to narrow it down, e.g. Button, MenuItem",
            ),
            Param::optional(
                "window",
                "Part of the window title (default: the window behind Vavis)",
            ),
            Param::optional(
                "index",
                "Which match, when several share the name (0 = first)",
            ),
        ]
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["düğmesine", "butonuna", "bas", "tıkla", "click", "press"]
    }

    fn run(&self, args: &Value) -> ToolOutcome {
        let Some(name) = arg_str(args, "name").filter(|n| !n.trim().is_empty()) else {
            return ToolOutcome::err("name gerekli");
        };
        let kind = arg_str(args, "type").unwrap_or("");
        let window = arg_str(args, "window").unwrap_or("");
        let nth = arg_num(args, "index").unwrap_or(0.0).max(0.0) as usize;

        let out = match run(&act_script(window, name.trim(), kind, nth, "invoke", "")) {
            Ok(out) => out,
            Err(e) => return ToolOutcome::err(format!("basılamadı: {e}")),
        };
        match parse_acted(&out) {
            Some(Acted::Done { how, label }) => {
                ToolOutcome::ok(format!("{label} — {how} ile basıldı"))
            }
            Some(Acted::Point { x, y, label }) => {
                let clicked = super::computer::click_at(x, y);
                if clicked.ok {
                    ToolOutcome::ok(format!("{label} — ({x},{y}) noktasına tıklandı"))
                } else {
                    clicked
                }
            }
            None => ToolOutcome::err(format!("beklenmeyen cevap: {}", first_line(&out))),
        }
    }
}

pub struct SetElementText;

impl Tool for SetElementText {
    fn name(&self) -> &'static str {
        "set_element_text"
    }

    fn description(&self) -> &'static str {
        "Fills a text field by its name (from list_ui_elements), replacing what is in \
         it. Faster and more exact than type_text, and needs no focus first."
    }

    fn domain(&self) -> Domain {
        Domain::Vision
    }

    fn risk(&self) -> Risk {
        Risk::Destructive
    }

    fn params(&self) -> Vec<Param> {
        vec![
            Param::required("name", "The field's name, or its automation id"),
            Param::required("text", "What to put in it"),
            Param::optional(
                "window",
                "Part of the window title (default: the window behind Vavis)",
            ),
        ]
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["alanına", "kutusuna", "doldur", "fill"]
    }

    fn run(&self, args: &Value) -> ToolOutcome {
        let Some(name) = arg_str(args, "name").filter(|n| !n.trim().is_empty()) else {
            return ToolOutcome::err("name gerekli");
        };
        let Some(text) = arg_str(args, "text") else {
            return ToolOutcome::err("text gerekli");
        };
        if text.chars().count() > MAX_SET_CHARS {
            return ToolOutcome::err(format!(
                "metin çok uzun (en fazla {MAX_SET_CHARS} karakter)"
            ));
        }
        let window = arg_str(args, "window").unwrap_or("");

        let out = match run(&act_script(window, name.trim(), "", 0, "settext", text)) {
            Ok(out) => out,
            Err(e) => return ToolOutcome::err(format!("yazılamadı: {e}")),
        };
        match parse_acted(&out) {
            Some(Acted::Done { label, .. }) => ToolOutcome::ok(format!(
                "{label} dolduruldu ({} karakter)",
                text.chars().count()
            )),
            // A field without a value pattern: click into it and type.
            Some(Acted::Point { x, y, label }) => {
                let clicked = super::computer::click_at(x, y);
                if !clicked.ok {
                    return clicked;
                }
                let typed = super::computer::type_text(text);
                if typed.ok {
                    ToolOutcome::ok(format!("{label} — tıklanıp yazıldı"))
                } else {
                    typed
                }
            }
            None => ToolOutcome::err(format!("beklenmeyen cevap: {}", first_line(&out))),
        }
    }
}

pub struct Scroll;

impl Tool for Scroll {
    fn name(&self) -> &'static str {
        "scroll"
    }

    fn description(&self) -> &'static str {
        "Scrolls with the mouse wheel, at a point or wherever the pointer is."
    }

    fn domain(&self) -> Domain {
        Domain::Vision
    }

    /// Scrolling changes what is shown, not what is stored.
    fn risk(&self) -> Risk {
        Risk::Moderate
    }

    fn params(&self) -> Vec<Param> {
        vec![
            Param::required("direction", "up | down | left | right"),
            Param::optional("amount", "Wheel notches, 1-20 (default 3)"),
            Param::optional("x", "Where to scroll, horizontal pixel"),
            Param::optional("y", "Where to scroll, vertical pixel"),
        ]
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["kaydır", "scroll", "aşağı in", "yukarı çık"]
    }

    fn run(&self, args: &Value) -> ToolOutcome {
        let direction = match arg_str(args, "direction").map(str::to_lowercase).as_deref() {
            Some("up" | "yukarı" | "yukari") => ScrollDir::Up,
            Some("down" | "aşağı" | "asagi") => ScrollDir::Down,
            Some("left" | "sol") => ScrollDir::Left,
            Some("right" | "sağ" | "sag") => ScrollDir::Right,
            _ => return ToolOutcome::err("direction: up | down | left | right"),
        };
        let amount = arg_num(args, "amount").unwrap_or(3.0).clamp(1.0, 20.0) as i32;
        let at = match (arg_num(args, "x"), arg_num(args, "y")) {
            (Some(x), Some(y)) => Some((x as i32, y as i32)),
            _ => None,
        };
        super::computer::scroll(direction, amount, at)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDir {
    Up,
    Down,
    Left,
    Right,
}

pub struct Drag;

impl Tool for Drag {
    fn name(&self) -> &'static str {
        "drag"
    }

    fn description(&self) -> &'static str {
        "Drags with the left mouse button held, from one point to another: moving a \
         file, a slider, a window, a selection."
    }

    fn domain(&self) -> Domain {
        Domain::Vision
    }

    fn risk(&self) -> Risk {
        Risk::Destructive
    }

    fn params(&self) -> Vec<Param> {
        vec![
            Param::required("from_x", "Start, horizontal pixel"),
            Param::required("from_y", "Start, vertical pixel"),
            Param::required("to_x", "End, horizontal pixel"),
            Param::required("to_y", "End, vertical pixel"),
        ]
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["sürükle", "drag", "taşı"]
    }

    fn run(&self, args: &Value) -> ToolOutcome {
        let coords = ["from_x", "from_y", "to_x", "to_y"].map(|k| arg_num(args, k));
        let [Some(fx), Some(fy), Some(tx), Some(ty)] = coords else {
            return ToolOutcome::err("from_x, from_y, to_x, to_y gerekli");
        };
        super::computer::drag((fx as i32, fy as i32), (tx as i32, ty as i32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quote_cannot_break_out_of_a_literal() {
        assert_eq!(ps_literal("it's"), "'it''s'");
        // Nothing inside single quotes is expanded by PowerShell.
        assert_eq!(ps_literal("$(Remove-Item C:\\)"), "'$(Remove-Item C:\\)'");
        assert_eq!(ps_literal("'; Stop-Computer; '"), "'''; Stop-Computer; '''");
    }

    #[test]
    fn every_user_value_reaches_the_script_as_a_literal() {
        let s = act_script("Not'epad", "Kay'det", "Button", 0, "invoke", "x'y");
        assert!(s.contains("$wanted = 'Not''epad'"));
        assert!(s.contains("$name = 'Kay''det'"));
        assert!(s.contains("$text = 'x''y'"));

        // The typographic apostrophe closes a PowerShell string too.
        let s = act_script("", "it\u{2019}s", "", 0, "invoke", "");
        assert!(s.contains("$name = 'it\u{2019}\u{2019}s'"), "{s}");
    }

    #[test]
    fn names_are_matched_as_text_not_as_wildcards() {
        // "Page [1]" is a character class to -like and never matched itself;
        // "Save [" threw.
        let s = act_script("", "Page [1]", "", 0, "invoke", "");
        assert!(s.contains("[WildcardPattern]::Escape($name)"));
        let s = list_script("", "[x]");
        assert!(s.contains("[WildcardPattern]::Escape($filter)"));
        assert!(prelude("Doc [1]").contains("[WildcardPattern]::Escape($wanted)"));
    }

    #[test]
    fn element_lines_parse_and_bad_ones_are_skipped() {
        let out = "#WINDOW Notepad\nButton\u{1f}Kaydet\u{1f}save\u{1f}812\u{1f}440\u{1f}True\n\
                   garbage line\nEdit\u{1f}\u{1f}15\u{1f}300\u{1f}200\u{1f}False\n";
        let els = parse_elements(out);
        assert_eq!(els.len(), 2);
        assert_eq!(els[0].name, "Kaydet");
        assert_eq!((els[0].x, els[0].y), (812, 440));
        assert!(els[0].enabled);
        assert!(!els[1].enabled);
    }

    #[test]
    fn the_list_reads_well_and_marks_disabled_and_unnamed() {
        let els = parse_elements(
            "Button\u{1f}Kaydet\u{1f}\u{1f}1\u{1f}2\u{1f}True\nEdit\u{1f}\u{1f}search\u{1f}3\u{1f}4\u{1f}False",
        );
        let text = format_elements("Notepad", &els);
        assert!(text.contains("- Button \"Kaydet\" @(1,2)"));
        assert!(text.contains("(adsız, id=search)"));
        assert!(text.contains("[devre dışı]"));
    }

    #[test]
    fn a_long_list_is_capped_and_says_so() {
        let many: String = (0..MAX_LISTED + 5)
            .map(|i| format!("Button\u{1f}b{i}\u{1f}\u{1f}0\u{1f}0\u{1f}True\n"))
            .collect();
        let text = format_elements("w", &parse_elements(&many));
        assert!(text.contains("5 kontrol daha"));
    }

    #[test]
    fn an_empty_window_suggests_a_screenshot() {
        assert!(format_elements("Game", &[]).contains("take_screenshot"));
    }

    #[test]
    fn both_act_results_parse() {
        assert_eq!(
            parse_acted("done\u{1f}invoke\u{1f}Button \"Kaydet\""),
            Some(Acted::Done {
                how: "invoke".into(),
                label: "Button \"Kaydet\"".into()
            })
        );
        assert_eq!(
            parse_acted("noise\npoint\u{1f}10\u{1f}20\u{1f}Edit \"Ara\""),
            Some(Acted::Point {
                x: 10,
                y: 20,
                label: "Edit \"Ara\"".into()
            })
        );
        assert_eq!(parse_acted("nothing"), None);
    }

    #[test]
    fn click_element_needs_a_name() {
        assert!(!ClickElement.run(&serde_json::json!({})).ok);
        assert!(!ClickElement.run(&serde_json::json!({"name": "  "})).ok);
    }

    #[test]
    fn scroll_needs_a_direction() {
        assert!(!Scroll.run(&serde_json::json!({})).ok);
        assert!(!Scroll.run(&serde_json::json!({"direction": "sideways"})).ok);
    }

    #[test]
    fn drag_needs_all_four_coordinates() {
        assert!(
            !Drag
                .run(&serde_json::json!({"from_x": 1, "from_y": 2, "to_x": 3}))
                .ok
        );
    }

    #[test]
    fn setting_text_refuses_an_essay() {
        let out = SetElementText.run(&serde_json::json!({
            "name": "x", "text": "a".repeat(MAX_SET_CHARS + 1)
        }));
        assert!(!out.ok);
    }

    #[test]
    fn only_listing_is_free_of_approval() {
        assert_eq!(ListElements.risk(), Risk::Safe);
        assert_eq!(ClickElement.risk(), Risk::Destructive);
        assert_eq!(SetElementText.risk(), Risk::Destructive);
        assert_eq!(Drag.risk(), Risk::Destructive);
    }
}
