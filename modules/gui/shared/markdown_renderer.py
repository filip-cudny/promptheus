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

_DARK_FORMATTER = HtmlFormatter(
    noclasses=True,
    style="monokai",
    nowrap=True,
    nobackground=True,
)

_CSS = f"""
body {{
    background-color: {COLOR_BUBBLE_TEXT_EDIT_BG};
    color: {COLOR_TEXT};
    font-family: -apple-system, "Segoe UI", "Helvetica Neue", Arial, sans-serif;
    font-size: 15px;
    line-height: 1.3;
    margin: 0;
    padding: 4px 0;
}}
h1, h2, h3, h4, h5, h6 {{
    color: {COLOR_TEXT_LIGHT_GRAY};
    margin: 12px 0 6px 0;
    font-weight: 600;
}}
h1 {{ font-size: 18px; }}
h2 {{ font-size: 16px; }}
h3 {{ font-size: 14px; }}
h4, h5, h6 {{ font-size: 13px; }}
p {{
    margin: 4px 0;
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
    font-size: 12px;
}}
a.copy-link {{
    color: {COLOR_TEXT_HINT};
    text-decoration: none;
    font-size: 13px;
    padding: 0px 2px;
}}
a.copy-link:hover {{
    color: {COLOR_TEXT_LIGHT_GRAY};
}}
table.code-header {{
    width: 100%;
    border-spacing: 0;
    margin: 0;
    padding: 0;
}}
td.lang-label {{
    color: {COLOR_TEXT_HINT};
    font-size: 10px;
    font-family: -apple-system, "Segoe UI", "Helvetica Neue", Arial, sans-serif;
    padding: 4px 8px;
    text-align: left;
}}
td.copy-cell {{
    text-align: right;
    padding: 4px 8px;
}}
div.code-body {{
    padding: 8px 12px;
    margin: 0 0 8px 0;
    font-family: "Menlo", "Monaco", "Consolas", monospace;
    font-size: 11px;
    line-height: 1.4;
    color: {COLOR_TEXT};
    white-space: pre;
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


@dataclass
class MarkdownResult:
    html: str = ""
    code_blocks: list[str] = field(default_factory=list)


class _DarkHTMLRenderer(HTMLRenderer):

    def __init__(self, highlight_code: bool = True):
        super().__init__()
        self.code_blocks: list[str] = []
        self.highlight_code = highlight_code

    def block_code(self, text: str, **attrs: Any) -> str:
        raw_code = text.rstrip("\n")
        info = attrs.get("info", "")
        lang = info.split()[0] if info else ""

        index = len(self.code_blocks)
        self.code_blocks.append(raw_code)

        copy_icon = (
            '<img src="data:image/svg+xml;utf8,'
            '<svg xmlns=&quot;http://www.w3.org/2000/svg&quot; width=&quot;14&quot; height=&quot;14&quot; '
            'viewBox=&quot;0 0 24 24&quot; fill=&quot;none&quot; stroke=&quot;%23888888&quot; '
            'stroke-width=&quot;2&quot; stroke-linecap=&quot;round&quot; stroke-linejoin=&quot;round&quot;>'
            '<rect width=&quot;14&quot; height=&quot;14&quot; x=&quot;8&quot; y=&quot;8&quot; rx=&quot;2&quot;/>'
            '<path d=&quot;M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2&quot;/>'
            '</svg>" width="14" height="14">'
        )
        copy_link = f'<a class="copy-link" href="copy-code:{index}">{copy_icon}</a>'
        lang_display = escape(lang)

        header_html = (
            f'<table class="code-header"><tr>'
            f'<td class="lang-label">{lang_display}</td>'
            f'<td class="copy-cell">{copy_link}</td>'
            f'</tr></table>'
        )

        if self.highlight_code and lang:
            try:
                lexer = get_lexer_by_name(lang, stripall=True)
                highlighted = highlight(raw_code, lexer, _DARK_FORMATTER)
                return f'{header_html}<div class="code-body">{highlighted}</div>\n'
            except ClassNotFound:
                pass

        if self.highlight_code and not lang and raw_code.strip():
            try:
                lexer = guess_lexer(raw_code)
                if lexer.analyse_text(raw_code) > 0.3:
                    highlighted = highlight(raw_code, lexer, _DARK_FORMATTER)
                    return f'{header_html}<div class="code-body">{highlighted}</div>\n'
            except ClassNotFound:
                pass

        return f'{header_html}<div class="code-body">{escape(raw_code)}</div>\n'


def render_markdown(text: str, highlight_code: bool = True) -> MarkdownResult:
    if not text or not text.strip():
        return MarkdownResult()

    renderer = _DarkHTMLRenderer(highlight_code=highlight_code)
    md = mistune.create_markdown(renderer=renderer, plugins=["strikethrough", "table"])

    html_body = md(text)

    full_html = f"<html><head><style>{_CSS}</style></head><body>{html_body}</body></html>"

    return MarkdownResult(html=full_html, code_blocks=renderer.code_blocks)
