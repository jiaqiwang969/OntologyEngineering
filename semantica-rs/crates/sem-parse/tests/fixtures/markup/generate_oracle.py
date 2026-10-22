"""Generate tests/fixtures/markup/oracle.json (and the corpus files) from the original Semantica.

Expected values for the html_parser / web_parser / xml_parser characterisation tests come from
running the original code here. Run from var/semantica-original/oracle/src:

    env -i HOME=../home PATH=../venv/bin:/usr/bin:/bin ../venv/bin/python \
        <crate>/tests/fixtures/markup/generate_oracle.py <crate>/tests/fixtures/markup

The script writes the corpus files (html/, xml/) and oracle.json into that directory. It makes
no network access and writes nothing else.
"""

import json
import os
import sys
from dataclasses import asdict
from pathlib import Path

sys.path.insert(0, os.getcwd())

import email.message  # noqa: E402,F401  (semantica.parse needs email.message loaded)

from semantica.utils.progress_tracker import get_progress_tracker  # noqa: E402

get_progress_tracker().displays = []

from semantica.parse import HTMLParser, XMLParser, WebParser, HTMLContentParser, JavaScriptRenderer  # noqa: E402

OUT_DIR = Path(sys.argv[1])
HTML_DIR = OUT_DIR / "html"
XML_DIR = OUT_DIR / "xml"
HTML_DIR.mkdir(parents=True, exist_ok=True)
XML_DIR.mkdir(parents=True, exist_ok=True)
REL = os.path.relpath


def err(e):
    return {"error": type(e).__name__, "message": str(e)}


def run(f, *a, **k):
    try:
        return {"ok": f(*a, **k)}
    except Exception as e:  # noqa: BLE001
        return err(e)


def data(d):
    return asdict(d)


BASE = "http://example.com/dir/page.html?x=1#top"

HTML = {
"01_basic.html": """<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>  Basic &amp; Simple Page </title>
  <meta name="Description" content="A page about things">
  <meta name="keywords" content="a, b, c">
  <meta name="author" content="Jane Doe">
  <meta property="og:title" content="OG Title">
  <meta property="og:description" content="OG Desc">
  <meta property="og:image" content="/img/og.png">
  <meta property="og:url" content="http://example.com/og">
  <meta name="twitter:card" content="summary">
  <meta name="" property="og:title" content="Second OG Title">
  <link rel="stylesheet" href="/style.css">
  <link rel="canonical" href="http://example.com/canonical">
  <style>body { color: red; } p > a { x: y; }</style>
  <script type="text/javascript">var x = 1 < 2 && "</div>";</script>
</head>
<body>
  <h1 id="main">Main   Heading</h1>
  <p>First paragraph with <a href="/rel/link" title="Rel">a relative link</a> and
     <a href="http://other.org/abs" rel="nofollow noopener">an absolute one</a>.</p>
  <h2>Second <em>level</em></h2>
  <p>Links: <a href="sub/page.html">sub</a> <a href="../up.html">up</a> <a href="?q=2">query</a>
     <a href="#frag">frag</a> <a href="//cdn.example.org/x.js">proto</a> <a href="mailto:a@b.c">mail</a>
     <a href="javascript:void(0)">js</a> <a href="">empty</a> <a name="anchor">no href</a></p>
  <h3 id="imgs">Images</h3>
  <img src="/img/a.png" alt="A" title="Image A" width="100" height="50">
  <img src="b.png">
  <img alt="no src">
  <img src="" alt="empty src">
  <noscript><a href="/noscript-link">ns</a><img src="/ns.png"></noscript>
  <!-- a comment -->
  <form action="/submit" method="POST">
    <input type="text" name="q" required>
    <input type="password" name="pw" value="secret">
    <input type="checkbox" name="cb" value="on" checked>
    <input type="hidden" name="h" value="v">
    <input type="" name="empty_type">
    <input name="no_type">
    <textarea name="ta">Some text</textarea>
    <select name="sel"><option value="1">One</option></select>
  </form>
  <form><input name="x"></form>
  <table>
    <thead><tr><th>Name</th><th>Age</th></tr></thead>
    <tbody><tr><td>Alice</td><td>30</td></tr><tr><td> Bob </td><td>25</td></tr></tbody>
  </table>
  <table><tr><th>H1</th><td>D1</td></tr><tr></tr><tr><td>D2</td></tr></table>
  <table><tr><td>outer<table><tr><td>inner</td></tr></table></td></tr></table>
  <ul><li>one</li><li>two</li></ul>
  <ol><li>first</li></ol>
  <h1>Another top heading</h1>
</body>
</html>
""",
"02_malformed.html": """<HTML><BODY CLASS="Main  Body" ID=top>
<div><p>one<p>two</div>
<span>stray</span></span></em>
<P>Upper <B>bold</B></P>
<a href=/unquoted/path title=hello>unquoted</a>
<a href="1" href="2" HREF="3">dup</a>
<br>after br</br>more<br/>
<img src="x.png"></img>text
<p/>self-closed p
<div class="a"><div class="b">nested</div>
<a href="x>text</a><b class="y">z</b>
</body></html>
trailing text after html
<div class="unterminated""",
"03_entities.html": """<p>amp: &amp; lt: &lt; gt: &gt; nbsp:[&nbsp;] copy no semi: &copy copy: &copy;</p>
<p>unknown: &unknownentity; &foo bar &AMP; &Eacute; &eacute &notit; &notin;</p>
<p>numeric: &#169; &#xA9; &#XA9; &#150; &#0; &#x110000; &#55296; &#65 &#x41 &#128512;</p>
<p>trailing &amp</p>
<p>lone & ampersand &; &#; &# end</p>
<p>first bad &#12ab; then <b>bold</b> and second bad &#zz; then <i>italic</i> end</p>
""",
"04_comments.html": """<!DOCTYPE html PUBLIC "-//W3C//DTD HTML 4.01//EN">
<html><body>
<!-- normal comment -->
<!-- spaced --!> after bang
<!---->empty
<!--->abrupt1
<!-->abrupt2
<! bogus decl >
</ bogus end>
</>
<?php echo 1 ?>
<?xml version="1.0"?>
<p><![CDATA[x<y & z]]></p>
<![if !IE]><p>conditional</p><![endif]>
<![weird stuff>
<!doctype other>
<p>a < b and c<1 and </ space</p>
<!-- unterminated comment at end
""",
"05_script_style.html": """<html><head>
<title>Title &amp; <b>not bold</b> &lt;x&gt;</title>
<script>if (a</b) { x = "</scr" + "ipt>"; }</SCRIPT >
<style>p>a{color:red}</style>
<script>
<!--
document.write("<p>hidden</p>");
//-->
</script>
</head><body>
<textarea name="t">  line1
  <p>not a tag</p> &lt; &amp;  </textarea>
<pre>  keep   spaces
    here  </pre>
<template><p>template text</p></template>
<ruby>漢<rp>(</rp><rt>kan</rt><rp>)</rp></ruby>
<p>   </p>
<p>
</p>
<code>  code   spaces </code>
</body></html>
""",
"06_meta.html": "﻿<html><head><meta charset=\"ISO-8859-1\">\r\n<meta http-equiv=\"Content-Type\" content=\"text/html; charset=ISO-8859-1\">\r\n<meta http-equiv=\"refresh\" content=\"5\">\r\n<title>Meta Test</title></head>\r\n<body>\r\n<p>Café — naïve text</p>\r\n<p>tab\tseparated　ideographic</p>\r\n<p>  multiple    spaces   </p>\r\n</body></html>\r\n",
"07_cdata_list.html": """<html><head>
<link rel="Canonical" href="/wrong-case">
<link rel="stylesheet canonical" href="/multi">
<link rel="canonical" href="/second">
</head><body>
<a href="/r" rel=" nofollow   external ">r</a>
<a href="/x" class="  a   b  c ">c</a>
<table><tr><td headers="h1 h2">cell</td></tr></table>
<form accept-charset="utf-8 latin1"><input name="i"></form>
<div accesskey="a b" data-x="1 2">d</div>
</body></html>
""",
"08_unterminated_script.html": """<html><body><p>before</p><script>var x = "<p>inside</p>";
// never closed
<p>after</p>
""",
"09_incomplete_end.html": """<div><p>ok</p></div>
<span>text &amp more</span>
<a href="#x">tail""",
"10_whitespace.html": """<html>
  <head>
    <title>

      Spaced   Title

    </title>
  </head>
  <body>


    <h1>   Lots   of   space   </h1>

    <p>line one
       line two</p>
    <p> nbsp </p>
  </body>
</html>
""",
}

out = {}

# ------------------------------------------------------------------ HTML files
files = {}
for name, content in HTML.items():
    path = HTML_DIR / name
    with open(path, "w", encoding="utf-8", newline="") as f:
        f.write(content)
    p = Path(path)
    hp = HTMLParser()
    cp = HTMLContentParser()
    rec = {}
    rec["parse"] = run(lambda: data(hp.parse(p, extract_forms=True)))
    rec["parse_base"] = run(lambda: data(hp.parse(p, base_url=BASE, extract_forms=True)))
    rec["parse_no_extract"] = run(lambda: data(hp.parse(p, extract_links=False, extract_images=False, extract_tables=False, clean_text=False)))
    rec["raw_text"] = run(hp.extract_text, p, clean=False)
    rec["metadata"] = run(hp.extract_metadata, p)
    rec["links_base"] = run(hp.extract_links, p, base_url=BASE)
    rec["clean_default"] = run(cp.clean_html, p)
    rec["clean_raw"] = run(cp.clean_html, p, remove_scripts=False, remove_comments=False, normalize_whitespace=False)
    rec["clean_keep_scripts"] = run(cp.clean_html, p, remove_scripts=False)
    rec["clean_keep_comments"] = run(cp.clean_html, p, remove_comments=False)
    rec["structured"] = run(cp.extract_structured_content, p)
    files[name] = rec
out["html_files"] = files

# ------------------------------------------------------------------ HTML strings
strings = [
    "<p>Hello <b>World</b></p>",
    "<html><head><title>T</title></head><body><a href='a.html'>A</a></body></html>",
    "plain text, no tags",
    "  ",
    "<p>&lt;escaped&gt;</p>",
    "",
    "x" * 300,
    "<p>" + "word " * 300 + "</p>",
]
str_cases = []
for s in strings:
    hp = HTMLParser()
    str_cases.append({
        "input": s,
        "parse": run(lambda: data(hp.parse(s))),
        "parse_base": run(lambda: data(hp.parse(s, base_url="http://example.com/a/b"))),
        "text": run(hp.extract_text, s),
        "raw_text": run(hp.extract_text, s, clean=False),
    })
out["html_strings"] = str_cases
out["missing_path"] = run(lambda: data(HTMLParser().parse(Path(HTML_DIR / "does-not-exist.html"))))

# ------------------------------------------------------------------ urljoin
bases = [
    "http://a/b/c/d;p?q",
    "http://example.com",
    "http://example.com/dir/",
    "https://user:pw@host:8080/path/file.html?x#y",
    "file:///tmp/dir/file.html",
    "ftp://ftp.example.org/pub/",
    "mailto:someone@example.com",
    "relative/base/",
]
hrefs = [
    "g:h", "g", "./g", "g/", "/g", "//g", "?y", "g?y", "#s", "g#s", "g?y#s", ";x", "g;x", "g;x?y#s",
    "", ".", "./", "..", "../", "../g", "../..", "../../", "../../g", "../../../g", "../../../../g",
    "/./g", "/../g", "g.", ".g", "g..", "..g", "./../g", "./g/.", "g/./h", "g/../h", "g;x=1/./y",
    "g;x=1/../y", "http:g", "https://other.org/x", "HTTP://UPPER.example/Path", "  spaced.html",
    "with\ttab.html", "a b c", "%7Euser", "ümläut", "http://[::1]/x", "http://[v1.fe]/x",
]
urljoin_cases = []
for b in bases:
    html = "".join('<a href="%s">%d</a>' % (h.replace('"', "&quot;"), i) for i, h in enumerate(hrefs))
    urljoin_cases.append({"base": b, "hrefs": hrefs, "links": run(HTMLParser().extract_links, html, base_url=b)})
for b, h in [("http://example.com/", "http://[::1"), ("http://example.com/", "http://[1.2.3.4]/"),
             ("http://example.com/", "http://[zz]/"), ("http://example.com/", "http://a／b/"),
             ("http://[::1", "x")]:
    html = '<a href="%s">x</a>' % h
    urljoin_cases.append({"base": b, "hrefs": [h], "links": run(HTMLParser().extract_links, html, base_url=b)})
out["urljoin"] = urljoin_cases

# The documents above are too long for `Path(html).exists()` (OSError); the same links from
# files exercise urljoin itself.
URL_DIR = OUT_DIR / "urljoin"
URL_DIR.mkdir(parents=True, exist_ok=True)
urljoin_files = []
for i, b in enumerate(bases):
    html = "".join('<a href="%s">%d</a>' % (h.replace('"', "&quot;"), j) for j, h in enumerate(hrefs))
    path = URL_DIR / ("links_%02d.html" % i)
    with open(path, "w", encoding="utf-8", newline="") as f:
        f.write(html)
    urljoin_files.append({"base": b, "file": path.name, "links": run(HTMLParser().extract_links, Path(path), base_url=b)})
out["urljoin_files"] = urljoin_files

# ------------------------------------------------------------------ web parser
wp = WebParser()
page = HTML_DIR / "01_basic.html"
web = {}
web["parse_html"] = run(lambda: data(wp.parse_web_content("<p>web <a href='/x'>x</a></p>", content_type="html", base_url="http://e.com/a/")))
web["parse_xml"] = run(lambda: data(wp.parse_web_content("<root><a>1</a></root>", content_type="xml")))
web["parse_other"] = run(lambda: data(wp.parse_web_content("{}", content_type="json")))
web["text_clean"] = run(wp.extract_text, "<p>Hello   <script>x</script>World</p>")
web["text_raw"] = run(wp.extract_text, "<p>Hello   <script>x</script>World</p>", clean=False)
web["text_render"] = run(wp.extract_text, "<p>Rendered <b>text</b></p>", render_javascript=True)
web["text_file"] = run(wp.extract_text, page)
web["links"] = run(wp.extract_links, "<a href='a'>1</a><a href='/b' rel='x'>2</a>", base_url="http://e.com/d/")
web["render_string"] = run(wp.render_javascript, "<p>no js</p>")
web["render_file"] = run(wp.render_javascript, page)
jr = JavaScriptRenderer()
web["js_execute"] = run(jr.execute_script, "return 1")
web["js_wait"] = run(jr.wait_for_content)
web["js_use_selenium"] = JavaScriptRenderer(use_selenium=True).use_selenium
web["js_selenium_render"] = run(JavaScriptRenderer(use_selenium=True).render_page, "<p>x</p>")
web["web_parser_config"] = {"js_use_selenium": WebParser(js={"use_selenium": False}).js_renderer.use_selenium}
hcp = HTMLContentParser()
web["content_structured"] = run(hcp.extract_structured_content, "<h1>T</h1><p>x</p><ul><li>a</li></ul>")
web["content_structured_file"] = run(hcp.extract_structured_content, page)
web["content_parse"] = run(lambda: data(hcp.parse("<p>x</p>")))
CLEAN_DOCS = [
    "<html><head><script>var a = 1;</script><style>p{}</style></head><body><!-- note --><p>  a   b\n c </p><pre>  keep   this  </pre><code> x  y </code><noscript>ns</noscript></body></html>",
    "<p>Tom &amp; Jerry &lt;3 &quot;q&quot;</p><br><img src='a.png' alt=\"x>y\">",
    "<meta charset='ISO-8859-1'><meta http-equiv='Content-Type' content='text/html; charset=latin-1'><p class='a  b'>t</p>",
    "<!DOCTYPE html><div>\n\n   </div><![CDATA[x]]><?php echo 1 ?>",
    "<table><tr><td> 1 </td></tr></table><textarea>  t  </textarea>",
]
for i, doc in enumerate(CLEAN_DOCS):
    web[f"clean_{i}"] = run(hcp.clean_html, doc)
    web[f"clean_{i}_keep"] = run(hcp.clean_html, doc, remove_scripts=False, remove_comments=False, normalize_whitespace=False)
web["clean_file"] = run(hcp.clean_html, page)
out["web"] = web

# ------------------------------------------------------------------ XML
XML_DOCS = [
 '<root><a x="1">t</a></root>',
 '<?xml version="1.0"?>\n<root>\n  <a>  hi  </a>\n</root>',
 '<root xmlns="http://d" xmlns:p="http://p"><p:a p:attr="v" b="c">x</p:a><b/></root>',
 '<root><!-- c --><a/></root>',
 '<!-- c --><root><a/></root><!-- after -->',
 '<root><?pi x?><a/></root>',
 '<root>a &amp; b &lt; &#65; &#x42;</root>',
 '<root>&foo;</root>',
 '<!DOCTYPE r [<!ENTITY e "ent">]><r>&e;</r>',
 '<root><![CDATA[x<y]]>z</root>',
 'not xml',
 '<a><b></a>',
 '<a>',
 '<a></a><b/>',
 '<a></a>junk',
 '<a b="1" b="2"/>',
 '<a b=1/>',
 '<1a/>',
 '<a>é中</a>',
 '<?xml version="1.0" encoding="ISO-8859-1"?><a>é</a>',
 '<a xmlns:x="u"><x:b/></a>',
 '<a><b>1</b>tail<c>2</c></a>',
 '  <a/>',
 '<a>&#0;</a>',
 '<a>&#x110000;</a>',
 '<root><x:a/></root>',
 '<a xmlns="http://d"><b xmlns=""/></a>',
 '<a>\x01</a>',
 "<a>\n<b>\n</c>",
 "<a>é</b>",
 "<a>éé<b></c>",
 "<é><b></c></é>",
 "\n\n  oops",
 "<a><!-- x -- y --></a>",
 "<a><!-- x </a>",
 "<a><![CDATA[ x </a>",
 "<a><?pi x </a>",
 "<a b='1></a>",
 "<a b></a>",
 "<a b=\"<\"/>",
 "<a>&</a>",
 "<a>& b</a>",
 "<a>&#xZZ;</a>",
 "<a>&#12</a>",
 "<a></a >",
 "<a></ a>",
 "<?xml version='2.0'?><a/>",
 "<?xml version='1.0' encoding='bogus'?><a/>",
 " <?xml version='1.0'?><a/>",
 "<a/><?xml version='1.0'?>",
 "<?XML version='1.0'?><a/>",
 "<a xmlns:xml='http://other'/>",
 "<a xmlns:p=''/>",
 "<a><b/><b/><c x='1' y='2' x='3'/></a>",
 "<!DOCTYPE a><a/>",
 "<!DOCTYPE a SYSTEM 'x.dtd'><a>&ext;</a>",
 "<!DOCTYPE a [<!ENTITY e '<b>in</b>'>]><a>&e;</a>",
 "<!DOCTYPE a [<!ENTITY e 'v'>]><a x='&e;'/>",
 "<a>]]></a>",
 "﻿<a/>",
 "<a>\t\r\n</a>",
 "<a:b xmlns:a='u'></a:b>",
 "<a></a:b>",
 "<a x='1'x='2'/>",
 "<a\n  b='1'\n  c=2/>",
 "<root>\n  <item>1</item>\n  <item>2</iten>\n</root>",
 "<root><x:a/></b>", "<root><x:a/><y:b/></root>", "<root><a x:y='1'/></root>",
 "   ", "\n", "﻿", "<?xml version='1.0'?>", "<!-- only -->", "<?pi x?>",
 "<a/>text", "<a/>  \n ", "<a/><!-- c -->", "<a/><!DOCTYPE a>", "<!DOCTYPE a><!DOCTYPE b><a/>",
 "<a x='&undefined;'/>", "<a x='&amp;&lt;&#65;'/>", "<a><!-- a<b --></a>", "<a><!-- x ---></a>",
 "<a xml:lang='en'/>", "<a xmlns='u'><b xmlns='v'/><c/></a>", "<a xmlns:p='u'><p:b xmlns:p='v'/></a>",
 "<a xmlns=''/>", "<p:a xmlns:p='u' xmlns='d' xmlns:q='w'/>", "<aé bé='1'/>",
 "<a>&lt;&gt;&amp;&apos;&quot;</a>", "<a>\r\n x \r y</a>", "<a b=' x\n\ty '/>",
 "<a>x</a>\n<!-- trailing -->\n", "<?xml version='1.0' encoding='utf-8' standalone='yes'?><a/>",
 "<?xml version='1.0' standalone='maybe'?><a/>",
 "<?xml version=\"1.0\"?>\n<!DOCTYPE note SYSTEM \"Note.dtd\">\n<note><to>T</to></note>",
 "<a><b>1<c/>2</b>3</a>", "<a/ >", "<a b='1' />", "<a b = '1'/>", "<a\tb='1'/>", "<a><![CDATA[]]></a>",
 "<a>&#x9;&#10;&#13;</a>", "<a>&#xFFFE;</a>", "<a>x<!-- c -->y</a>",
 "<p:a xmlns:p='u' xmlns:q='u' p:x='1' q:x='2'/>",
 "<!DOCTYPE a [<!ENTITY e1 'one'><!ENTITY e2 '&e1; two'>]><a x='&e2;'>&e2;</a>",
 "<!DOCTYPE a [<!ENTITY e '&#60;b&#62;x&#60;/b&#62;'>]><a>&e;</a>",
 "<a b='1' c=\"2\" d='&quot;'/>",
 "<a><b><c>deep</c></b></a>",
 "<a xmlns:p='u'><b p:x='1'/></a>",
]
xml_cases = []
for s in XML_DOCS:
    rec = {"input": s}
    for engine in ("lxml", "etree"):
        rec[engine] = run(lambda: data(XMLParser().parse(s, engine=engine)))
    rec["to_dict"] = run(XMLParser().to_dict, s)
    rec["by_tag_b"] = run(lambda: [asdict(e) for e in XMLParser().extract_by_tag(s, "b")])
    rec["no_ns"] = run(lambda: data(XMLParser().parse(s, extract_namespaces=False)))
    xml_cases.append(rec)
out["xml_cases"] = xml_cases

XPATH_DOC = "<root a='1' xml:lang='en'><item id='1'>x</item><item id='2'><sub>y</sub></item><other n='3'>  z  </other><n:q xmlns:n='http://n'/></root>"
XPATH_DOC_C = "<root a='1'><item id='1'>x</item><item id='2'><sub>y</sub></item><!-- c --><other/></root>"
NS_DOC = "<root xmlns:p='http://p'><p:item>1</p:item><item>2</item><p:item k='v'>3</p:item></root>"
DEF_NS_DOC = "<root xmlns='http://d' xmlns:p='http://p'><p:item>1</p:item><item>2</item></root>"
XPATHS = ["//item", "item", "/root/item[2]", "//item[@id='2']/sub", "//text()", "//@id", "count(//item)",
          "string(//item)", "//item[1]/following-sibling::*", "//comment()", "/", ".", "//*[local-name()='sub']/..",
          "//item[last()]", "//nonexistent", "boolean(//item)", "//item[", "//q:item", "foo(1)", "$x",
          "//item | //other", "//*", "descendant::*[2]", "//item[position() < 2]", "//item[contains(., 'y')]",
          "//sub/ancestor::*", "/root/@a", "//item/@*", "id('1')", "//item[@id > 1]", "//item[not(@id = '1')]",
          "//*[starts-with(name(), 'it')]", "//item[string-length(.) = 1]", "//other[normalize-space(.) = 'z']",
          "//*[@n * 2 = 6]", "//item[1] | //item[2]", "(//item)[2]", "//item[2]/preceding-sibling::item",
          "//sub/parent::item", "//sub/ancestor-or-self::*[1]", "//root//sub", "/descendant::sub",
          "//*[lang('en')]", "//item[@id='1' or @id='2'][2]", "//item[number(@id) mod 2 = 0]",
          "sum(//item/@id)", "string(//item/@id)", "concat('a','b')", "''", "//item[translate(., 'x', 'X') = 'X']",
          "//*[local-name() = 'q']", "//n:q", "//item[1]/following::*", "//other/preceding::*",
          "//item[substring-before('a-b', '-') = 'a']", "//item[round(1.5) = 2]", "//item[floor(@id div 2) = 1]",
          "child::item[attribute::id='1']", "self::root", "self::node()/item", "//processing-instruction()"]
xpath_cases = []
for d, engine in [(XPATH_DOC, "lxml"), (XPATH_DOC_C, "etree"), (NS_DOC, "lxml"), (DEF_NS_DOC, "lxml"), (DEF_NS_DOC, "etree"), (NS_DOC, "etree")]:
    exprs = XPATHS if d in (XPATH_DOC, XPATH_DOC_C) else ["//p:item", "//item", "//*", "//p:*", "//p:item[@k]"]
    for x in exprs:
        xpath_cases.append({"doc": d, "engine": engine, "xpath": x,
                            "result": run(lambda: [asdict(e) for e in XMLParser().find_elements(d, x, engine=engine)])})
out["xpath_cases"] = xpath_cases

# XML files
XML_FILES = {
    "catalog.xml": '<?xml version="1.0" encoding="UTF-8"?>\n<catalog xmlns:dc="http://purl.org/dc/elements/1.1/">\n  <book id="b1">\n    <dc:title>First</dc:title>\n    <price currency="EUR">10.50</price>\n  </book>\n  <book id="b2">\n    <dc:title>Second &amp; Last</dc:title>\n    <price currency="USD">7</price>\n  </book>\n</catalog>\n',
    "latin1.xml": '<?xml version="1.0" encoding="ISO-8859-1"?>\n<doc><name>Café</name></doc>\n',
    "crlf.xml": '<doc>\r\n  <line>one</line>\r\n  <line>two</line>\r\n</doc>\r\n',
}
xml_files = {}
for name, content in XML_FILES.items():
    path = XML_DIR / name
    with open(path, "w", encoding="utf-8", newline="") as f:
        f.write(content)
    p = Path(path)
    xml_files[name] = {
        "lxml": run(lambda: data(XMLParser().parse(p))),
        "etree": run(lambda: data(XMLParser().parse(p, engine="etree"))),
        "to_dict": run(XMLParser().to_dict, p),
        "find": run(lambda: [asdict(e) for e in XMLParser().find_elements(p, "//book[@id='b2'] | //line[2]")]),
        "find_ns": run(lambda: [asdict(e) for e in XMLParser().find_elements(p, "//dc:title")]),
        "by_tag": run(lambda: [asdict(e) for e in XMLParser().extract_by_tag(p, "title")]),
        "str_path": run(lambda: data(XMLParser().parse(str(p)))),
    }
out["xml_files"] = xml_files
out["xml_missing_path"] = run(lambda: data(XMLParser().parse(Path(XML_DIR / "missing.xml"))))
out["xml_long_string"] = run(lambda: data(XMLParser().parse("<a>" + "x" * 400 + "</a>")))

# Make paths in the output relative to the fixture directory.
text = json.dumps(out, ensure_ascii=False, indent=1, sort_keys=False)
text = text.replace(str(OUT_DIR.resolve()) + "/", "@FIXTURES@/").replace(str(OUT_DIR) + "/", "@FIXTURES@/")
with open(OUT_DIR / "oracle.json", "w", encoding="utf-8") as f:
    f.write(text)
print("wrote", OUT_DIR / "oracle.json", len(text))
