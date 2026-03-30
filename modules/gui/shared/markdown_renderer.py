"""Markdown-to-HTML conversion for assistant message rendering."""

from dataclasses import dataclass, field
from html import escape
from typing import Any

import mistune
from mistune.renderers.html import HTMLRenderer
from pygments import highlight
from pygments.formatters import HtmlFormatter
from pygments.lexers import get_lexer_by_name, guess_lexer
from pygments.util import ClassNotFound

from modules.gui.shared.theme import (
    COLOR_BUBBLE_TEXT_EDIT_BG,
    COLOR_BUTTON_BG,
    COLOR_BUTTON_HOVER,
    COLOR_SELECTION,
    COLOR_TEXT,
    COLOR_TEXT_HINT,
    COLOR_TEXT_LIGHT_GRAY,
    COLOR_TEXT_SECONDARY,
)

_DEFAULT_CODE_THEME = "paraiso-dark"


def _get_code_theme() -> str:
    try:
        from modules.utils.config import ConfigService

        return ConfigService().get_settings_data().get("code_theme", _DEFAULT_CODE_THEME)
    except Exception:
        return _DEFAULT_CODE_THEME


def _create_formatter(style: str = "") -> HtmlFormatter:
    return HtmlFormatter(
        noclasses=True,
        style=style or _get_code_theme(),
        nowrap=True,
        nobackground=True,
    )

_CSS = f"""
body {{
    background-color: {COLOR_BUBBLE_TEXT_EDIT_BG};
    color: {COLOR_TEXT};
    margin: 0;
    padding: 4px 0;
}}
h1, h2, h3, h4, h5, h6 {{
    color: {COLOR_TEXT_LIGHT_GRAY};
    margin: 12px 0 6px 0;
    font-weight: 600;
}}
h1 {{ font-size: 1.3em; }}
h2 {{ font-size: 1.15em; }}
h3 {{ font-size: 1.0em; }}
h4, h5, h6 {{ font-size: 0.95em; }}
p {{
    margin-top: 0;
    margin-bottom: 1em;
}}
ul, ol {{
    margin: 6px 0;
    padding-left: 24px;
}}
li {{
    margin: 2px 0;
}}
strong {{
    color: {COLOR_TEXT_LIGHT_GRAY};
    font-weight: 600;
}}
em {{
    font-style: italic;
}}
code {{
    background-color: #2d2d2d;
    color: #e06c75;
    padding: 1px 4px;
    border-radius: 3px;
    font-family: "Menlo", "Monaco", "Consolas", monospace;
    font-size: 0.85em;
}}
a.copy-link, a.copy-link:visited, a.copy-link:active, a.copy-link:focus {{
    color: {COLOR_TEXT_HINT};
    text-decoration: none;
    padding: 0px;
    outline: none;
    border: none;
    background: transparent;
}}
a.copy-link:hover {{
    color: {COLOR_TEXT_LIGHT_GRAY};
}}
table.code-wrapper {{
    border: none;
    border-spacing: 0;
    margin: 6px 0;
    background-color: #222222;
}}
td.code-body {{
    padding: 10px 14px;
    font-family: "Menlo", "Monaco", "Consolas", monospace;
    font-size: 0.85em;
    line-height: 1.4;
    color: {COLOR_TEXT};
    white-space: pre;
}}
table.code-toolbar {{
    border: none;
    border-spacing: 0;
    margin: 0 0 4px 0;
}}
td.lang-label {{
    color: {COLOR_TEXT_SECONDARY};
    font-size: 0.85em;
    font-family: "Menlo", "Monaco", "Consolas", monospace;
    padding: 0;
}}
td.copy-cell {{
    text-align: right;
    padding: 0;
}}
blockquote {{
    border-left: 3px solid {COLOR_TEXT_HINT};
    margin: 6px 0;
    padding: 4px 12px;
    color: {COLOR_TEXT_SECONDARY};
}}
hr {{
    border: none;
    border-top: 1px solid {COLOR_TEXT_HINT};
    margin: 12px 0;
}}
a {{
    color: {COLOR_SELECTION};
}}
"""


_LANG_DISPLAY_NAMES = {
    "ts": "TypeScript",
    "tsx": "TypeScript (JSX)",
    "js": "JavaScript",
    "jsx": "JavaScript (JSX)",
    "py": "Python",
    "rb": "Ruby",
    "rs": "Rust",
    "go": "Go",
    "java": "Java",
    "kt": "Kotlin",
    "cs": "C#",
    "cpp": "C++",
    "c": "C",
    "sh": "Shell",
    "bash": "Bash",
    "zsh": "Zsh",
    "ps1": "PowerShell",
    "sql": "SQL",
    "html": "HTML",
    "css": "CSS",
    "scss": "SCSS",
    "json": "JSON",
    "yaml": "YAML",
    "yml": "YAML",
    "toml": "TOML",
    "xml": "XML",
    "md": "Markdown",
    "dockerfile": "Dockerfile",
    "swift": "Swift",
    "php": "PHP",
    "lua": "Lua",
    "r": "R",
    "dart": "Dart",
    "zig": "Zig",
    "elixir": "Elixir",
    "ex": "Elixir",
    "erl": "Erlang",
    "hs": "Haskell",
    "scala": "Scala",
    "clj": "Clojure",
    "vim": "Vim Script",
    "graphql": "GraphQL",
    "proto": "Protobuf",
    "tf": "Terraform",
}


@dataclass
class MarkdownResult:
    html: str = ""
    code_blocks: list[str] = field(default_factory=list)


class _DarkHTMLRenderer(HTMLRenderer):

    def __init__(self, highlight_code: bool = True, confirmed_copy_index: int = -1):
        super().__init__()
        self.code_blocks: list[str] = []
        self.highlight_code = highlight_code
        self._confirmed_copy_index = confirmed_copy_index

    def softbreak(self) -> str:
        return "<br>\n"

    def block_code(self, text: str, **attrs: Any) -> str:
        raw_code = text.rstrip("\n")
        info = attrs.get("info", "")
        lang = info.split()[0] if info else ""

        index = len(self.code_blocks)
        self.code_blocks.append(raw_code)

        if index == self._confirmed_copy_index:
            icon = (
                '<img src="data:image/svg+xml;utf8,'
                '<svg xmlns=&quot;http://www.w3.org/2000/svg&quot; width=&quot;16&quot; height=&quot;16&quot; '
                'viewBox=&quot;0 0 24 24&quot; fill=&quot;none&quot; stroke=&quot;%23888888&quot; '
                'stroke-width=&quot;2&quot; stroke-linecap=&quot;round&quot; stroke-linejoin=&quot;round&quot;>'
                '<polyline points=&quot;20 6 9 17 4 12&quot;/>'
                '</svg>" width="16" height="16">'
            )
        else:
            icon = (
                '<img src="data:image/svg+xml;utf8,'
                '<svg xmlns=&quot;http://www.w3.org/2000/svg&quot; width=&quot;16&quot; height=&quot;16&quot; '
                'viewBox=&quot;0 0 24 24&quot; fill=&quot;none&quot; stroke=&quot;%23888888&quot; '
                'stroke-width=&quot;2&quot; stroke-linecap=&quot;round&quot; stroke-linejoin=&quot;round&quot;>'
                '<rect width=&quot;14&quot; height=&quot;14&quot; x=&quot;8&quot; y=&quot;8&quot; rx=&quot;2&quot;/>'
                '<path d=&quot;M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2&quot;/>'
                '</svg>" width="16" height="16">'
            )
        copy_link = f'<a class="copy-link" href="copy-code:{index}">{icon}</a>'
        lang_display = escape(_LANG_DISPLAY_NAMES.get(lang.lower(), lang))

        toolbar = (
            f'<table class="code-toolbar" width="100%"><tr>'
            f'<td class="lang-label">{lang_display}</td>'
            f'<td class="copy-cell">{copy_link}</td>'
            f'</tr></table>'
        )

        def _wrap_code_block(code_html: str) -> str:
            return (
                f'<table class="code-wrapper" width="100%"><tr>'
                f'<td class="code-body">{toolbar}{code_html}</td>'
                f'</tr></table>\n'
            )

        if self.highlight_code and lang:
            try:
                lexer = get_lexer_by_name(lang, stripall=True)
                highlighted = highlight(raw_code, lexer, _create_formatter())
                return _wrap_code_block(highlighted)
            except ClassNotFound:
                pass

        if self.highlight_code and not lang and raw_code.strip():
            try:
                lexer = guess_lexer(raw_code)
                if lexer.analyse_text(raw_code) > 0.3:
                    highlighted = highlight(raw_code, lexer, _create_formatter())
                    return _wrap_code_block(highlighted)
            except ClassNotFound:
                pass

        return _wrap_code_block(escape(raw_code))


def render_markdown(
    text: str, highlight_code: bool = True, confirmed_copy_index: int = -1
) -> MarkdownResult:
    if not text or not text.strip():
        return MarkdownResult()

    renderer = _DarkHTMLRenderer(
        highlight_code=highlight_code, confirmed_copy_index=confirmed_copy_index
    )
    md = mistune.create_markdown(renderer=renderer, plugins=["strikethrough", "table"])

    html_body = md(text)

    full_html = f"<html><head><style>{_CSS}</style></head><body>{html_body}</body></html>"

    return MarkdownResult(html=full_html, code_blocks=renderer.code_blocks)
