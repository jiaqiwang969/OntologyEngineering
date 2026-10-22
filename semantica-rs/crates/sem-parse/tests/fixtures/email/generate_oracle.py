"""Generate tests/fixtures/email/{*.eml, oracle.json} from the original Semantica (the oracle).

Writes the .eml corpus next to this script, then runs the original EmailParser / MIMEParser /
EmailThreadAnalyzer and the `email` package functions they rely on, recording every result
(or exception type and message) in oracle.json. Run from var/semantica-original/oracle/src:

    env -i HOME=../home PATH=../venv/bin:/usr/bin:/bin ../venv/bin/python \
        <crate>/tests/fixtures/email/generate_oracle.py <crate>/tests/fixtures/email

No network, no credentials; writes only into the given directory.
"""

import binascii
import dataclasses
import json
import os
import sys

sys.path.insert(0, os.getcwd())

import email.message  # noqa: E402,F401  (semantica.parse needs email.message loaded)
from email import message_from_bytes, message_from_string  # noqa: E402
from email._encoded_words import decode_b  # noqa: E402
from email.header import Header, decode_header  # noqa: E402
from email.utils import parsedate_to_datetime  # noqa: E402

from semantica.utils.progress_tracker import get_progress_tracker  # noqa: E402

get_progress_tracker().displays = []

from semantica.parse.email_parser import (  # noqa: E402
    EmailParser,
    EmailThreadAnalyzer,
    MIMEParser,
)

OUT_DIR = sys.argv[1]
OUT = {}


def err(e):
    return {"error": type(e).__name__, "message": str(e)}


def has_sur(s):
    try:
        s.encode()
        return False
    except UnicodeEncodeError:
        return True


def jsonable(x):
    if isinstance(x, dict):
        return {str(k): jsonable(v) for k, v in x.items()}
    if isinstance(x, (list, tuple)):
        return [jsonable(v) for v in x]
    if isinstance(x, (bytes, bytearray)):
        return list(x)
    if isinstance(x, str):
        return {"surrogates": list(x.encode("utf-8", "surrogateescape"))} if has_sur(x) else x
    if x is None or isinstance(x, (bool, int, float)):
        return x
    return str(x)


def strj(s):
    """A str that may carry surrogate escapes."""
    if s is None:
        return None
    if has_sur(s):
        return {"bytes": list(s.encode("ascii", "surrogateescape"))}
    return s


def safe(f):
    try:
        return {"ok": jsonable(f())}
    except Exception as e:  # noqa: BLE001
        return err(e)


def describe(m):
    d = {}
    d["content_type"] = m.get_content_type()
    d["default_type"] = m.get_default_type()
    d["is_multipart"] = m.is_multipart()
    d["unixfrom"] = strj(m.get_unixfrom())
    d["headers"] = [[k, str(v)] for k, v in m.items()]
    d["header_kinds"] = ["header" if isinstance(v, Header) else "str" for _, v in m.items()]
    d["preamble"] = strj(m.preamble)
    d["epilogue"] = strj(m.epilogue)
    d["defects"] = [type(x).__name__ for x in m.defects]
    d["charset"] = safe(m.get_content_charset)
    d["filename"] = safe(m.get_filename)
    d["boundary"] = safe(m.get_boundary)
    if m.is_multipart():
        d["payload_text"] = {"ok": None}
    else:
        d["payload_text"] = safe(lambda: m.get_payload())
    pd = m.get_payload(decode=True)
    d["payload_decoded"] = None if pd is None else list(pd)
    d["parts"] = [describe(p) for p in m.get_payload()] if m.is_multipart() else None
    return d


def email_data_json(ed):
    return jsonable(dataclasses.asdict(ed))


# ------------------------------------------------------------------------- corpus
CORPUS = {}

CORPUS["simple.eml"] = b"""From: Alice <alice@example.com>
To: Bob <bob@example.com>, carol@example.com
Cc: dave@example.com
Subject: Quarterly report
Date: Mon, 15 Jan 2024 10:30:00 +0000
Message-ID: <msg-001@example.com>
X-Mailer: TestMailer 1.0

Hello Bob,

Please find the report below.
"""

CORPUS["alternative.eml"] = b"""From: news@example.com
To: reader@example.com
Subject: Newsletter
Date: Tue, 16 Jan 2024 08:00:00 -0500
MIME-Version: 1.0
Content-Type: multipart/alternative; boundary="ALT"

This is a multi-part message in MIME format.
--ALT
Content-Type: text/plain; charset="utf-8"
Content-Transfer-Encoding: quoted-printable

Caf=C3=A9 news: a very long line that is soft-wrapped by the quoted printable =
encoder and continues here.
--ALT
Content-Type: text/html; charset=utf-8
Content-Transfer-Encoding: quoted-printable

<p>Caf=C3=A9 <b>news</b></p>
--ALT--
Epilogue text
"""

CORPUS["mixed_attachments.eml"] = b"""From: sender@example.com
To: recipient@example.com
Subject: Files attached
Date: Wed, 17 Jan 2024 12:00:00 +0100
Message-ID: <att@example.com>
MIME-Version: 1.0
Content-Type: multipart/mixed; boundary="MIX"

--MIX
Content-Type: text/plain; charset=us-ascii

See attachments.
--MIX
Content-Type: application/pdf; name="report.pdf"
Content-Disposition: attachment; filename="report.pdf"
Content-Transfer-Encoding: base64

JVBERi0xLjQKJcOkw7zDtsOfCjEgMCBvYmoKPDwvVHlwZS9DYXRhbG9nPj4KZW5kb2JqCg==
--MIX
Content-Type: text/plain; charset=utf-8
Content-Disposition: attachment; filename*=UTF-8''%E2%82%AC%20rates.txt
Content-Transfer-Encoding: quoted-printable

=E2=82=AC 1.10
--MIX
Content-Type: image/png
Content-Disposition: attachment; filename="=?utf-8?b?w6l0w6kucG5n?="
Content-Transfer-Encoding: base64

iVBORw0KGgo=
--MIX
Content-Type: application/octet-stream
Content-Disposition: attachment
Content-Transfer-Encoding: base64

AAEC
--MIX
Content-Type: image/gif; name="inline.gif"
Content-Disposition: inline; filename="inline.gif"
Content-Transfer-Encoding: base64

R0lGODlh
--MIX
Content-Type: application/zip
Content-Disposition: ATTACHMENT; filename="upper.zip"

PK
--MIX--
"""

CORPUS["nested.eml"] = b"""From: a@example.com
To: b@example.com
Subject: Nested parts
Content-Type: multipart/mixed; boundary="outer"

Preamble line one
Preamble line two

--outer
Content-Type: multipart/alternative; boundary="inner"

--inner
Content-Type: text/plain

Inner plain
--inner
Content-Type: text/html

<i>Inner html</i>
--inner--
inner epilogue

--outer
Content-Type: text/csv; name=data.csv
Content-Disposition: attachment; filename=data.csv

a,b
1,2

--outer--

outer epilogue
"""

CORPUS["rfc822.eml"] = b"""From: fwd@example.com
To: you@example.com
Subject: Fwd: original
Content-Type: multipart/mixed; boundary="B1"

--B1
Content-Type: text/plain

Forwarding this.
--B1
Content-Type: message/rfc822
Content-Disposition: attachment; filename="original.eml"

From: orig@example.com
To: fwd@example.com
Subject: original
Date: Thu, 18 Jan 2024 09:15:00 +0200

The original body.
--B1--
"""

CORPUS["raw_utf8_subject.eml"] = (
    "From: a@example.com\nTo: b@example.com\nSubject: Café menu\n\nBody\n"
).encode("utf-8")

CORPUS["raw_utf8_to.eml"] = (
    "From: a@example.com\nTo: Jörg <joerg@example.com>, b@example.com\n"
    "Subject: Hi\nMessage-ID: <raw@example.com>\n\nBody\n"
).encode("utf-8")

CORPUS["raw_utf8_body.eml"] = (
    "From: a@example.com\nSubject: Body charset\nContent-Type: text/plain; charset=utf-8\n"
    "Content-Transfer-Encoding: 8bit\n\nnaïve café ☕\n"
).encode("utf-8")

CORPUS["latin1_body.eml"] = (
    "From: a@example.com\nSubject: Latin\nContent-Type: text/plain; charset=iso-8859-1\n\n"
    "Grüße aus Köln\n"
).encode("latin-1")

CORPUS["no_charset_body.eml"] = (
    "From: a@example.com\nSubject: Default charset\n\nok é ÿ\n"
).encode("latin-1")

CORPUS["encoded_subjects.eml"] = b"""From: =?iso-8859-1?q?Andr=E9?= <andre@example.com>
To: =?utf-8?b?5byg5LiJ?= <zhang@example.com>
Subject: =?utf-8?q?Caf=C3=A9_au_lait?= =?utf-8?q?_et_cr=C3=A8me?=
X-Mixed: =?utf-8?b?5L2g5aW9?= world =?ISO-8859-1?Q?=E9t=E9?=
X-Plain-Then-Encoded: Hello =?utf-8?q?W=C3=B6rld?=
X-Encoded-Then-Plain: =?utf-8?q?W=C3=B6rld?= says hi
X-Lower-Charset: =?UTF-8?B?w6TDtsO8?=
Date: Fri, 19 Jan 2024 18:45:30 +0900

Encoded headers.
"""

CORPUS["folded.eml"] = (
    b"From: folder@example.com\r\n"
    b"To: one@example.com,\r\n two@example.com,\r\n\tthree@example.com\r\n"
    b"Subject: This is a long subject\r\n that continues on the next line\r\n"
    b"References: <a@x>\r\n <b@x>\r\n\t<c@x>\r\n"
    b"In-Reply-To: <b@x>\r\n"
    b"Message-ID: <folded@x>\r\n"
    b"Date: Sat, 20 Jan 2024 07:00:00 +0000\r\n"
    b"\r\n"
    b"CRLF body line one\r\n"
    b"line two\r\n"
)

CORPUS["dates.eml"] = b"""From: d@example.com
Subject: Dates
Date: not a date at all

Body
"""

CORPUS["date_invalid_day.eml"] = b"""From: d@example.com
Subject: Dates
Date: Mon, 32 Jan 2024 10:00:00 +0000

Body
"""

CORPUS["date_naive.eml"] = b"""From: d@example.com
Subject: Dates
Date: 1 Jan 2024 10:00:00 -0000

Body
"""

CORPUS["date_est.eml"] = b"""From: d@example.com
Subject: Dates
Date: Sun, 21 Jan 24 23:59 EST

Body
"""

CORPUS["unknown_charset.eml"] = b"""From: u@example.com
Subject: Unknown charset
Content-Type: text/plain; charset="x-unknown-charset"

Body
"""

CORPUS["odd_casing.eml"] = b"""SUBJECT: Upper case subject
from: lower@example.com
Message-Id: <odd@example.com>
TO: someone@example.com
content-type: TEXT/PLAIN; CHARSET=UTF-8

Body text
"""

CORPUS["duplicates.eml"] = b"""Received: from a by b; Mon, 15 Jan 2024 10:00:00 +0000
Received: from c by d; Mon, 15 Jan 2024 10:00:01 +0000
Subject: First subject
Subject: Second subject
X-Dup: one
X-Dup: two
From: dup@example.com

Body
"""

CORPUS["uuencode.eml"] = b"""From: uu@example.com
Subject: uu
Content-Type: text/plain
Content-Transfer-Encoding: x-uuencode

begin 644 hello.txt
,:&5L;&\\@=V]R;&0*
`
end
"""

CORPUS["base64_bad.eml"] = b"""From: b@example.com
Subject: bad base64
Content-Type: text/plain
Content-Transfer-Encoding: base64

SGVsbG8g!!d29ybGQ
"""

CORPUS["base64_unpadded.eml"] = b"""From: b@example.com
Subject: unpadded
Content-Type: text/plain
Content-Transfer-Encoding: BASE64

SGVsbG8gd29ybGQ
"""

CORPUS["no_boundary.eml"] = b"""From: n@example.com
Subject: No boundary
Content-Type: multipart/mixed

Just text in a multipart without a boundary.
"""

CORPUS["start_boundary_missing.eml"] = b"""From: s@example.com
Subject: Start boundary missing
Content-Type: multipart/mixed; boundary="XYZ"

No boundary lines at all here.
"""

CORPUS["close_boundary_missing.eml"] = b"""From: c@example.com
Subject: Close boundary missing
Content-Type: multipart/mixed; boundary="CB"

--CB
Content-Type: text/plain

Part without close
"""

CORPUS["unixfrom.eml"] = b"""From sender@example.com Mon Jan 15 10:00:00 2024
From: sender@example.com
Subject: Has envelope

Body after envelope
"""

CORPUS["digest.eml"] = b"""From: digest@example.com
Subject: Digest
Content-Type: multipart/digest; boundary="DG"

--DG

From: one@example.com
Subject: First

First digest message
--DG

From: two@example.com
Subject: Second

Second digest message
--DG--
"""

CORPUS["delivery_status.eml"] = b"""From: MAILER-DAEMON@example.com
Subject: Delivery Status Notification
Content-Type: multipart/report; report-type=delivery-status; boundary="DS"

--DS
Content-Type: text/plain

Delivery failed.
--DS
Content-Type: message/delivery-status

Reporting-MTA: dns; mx.example.com

Final-Recipient: rfc822; nobody@example.com
Action: failed
Status: 5.1.1
--DS--
"""

CORPUS["html_only.eml"] = b"""From: h@example.com
Subject: HTML only
Content-Type: text/html; charset=utf-8

<html><body><h1>Hi</h1></body></html>
"""

CORPUS["disposition_raw_utf8.eml"] = (
    "From: r@example.com\nSubject: Raw disposition\nContent-Type: multipart/mixed; boundary=\"R\"\n\n"
    "--R\nContent-Type: text/plain\n\nBody\n"
    "--R\nContent-Type: application/pdf\nContent-Disposition: attachment; filename=\"résumé.pdf\"\n\nPDF\n"
    "--R--\n"
).encode("utf-8")

CORPUS["empty.eml"] = b""

CORPUS["headers_only.eml"] = b"""From: only@example.com
Subject: Headers only
"""

CORPUS["missing_separator.eml"] = b"""From: m@example.com
Subject: Missing separator
This line is body text without a blank line.
Second body line.
"""

CORPUS["name_param.eml"] = b"""From: n@example.com
Subject: name parameter
Content-Type: multipart/mixed; boundary="NP"

--NP
Content-Type: application/vnd.ms-excel; name="sheet.xls"
Content-Disposition: attachment
Content-Transfer-Encoding: base64

0M8R4KGxGuE=
--NP--
"""

CORPUS["text_attachment.eml"] = b"""From: t@example.com
Subject: text attachment
Content-Type: multipart/mixed; boundary="TA"

--TA
Content-Type: text/plain

Main body
--TA
Content-Type: text/plain; charset=utf-8
Content-Disposition: attachment; filename="notes.txt"

Attached notes
--TA--
"""

CORPUS["rfc2231_continuations.eml"] = b"""From: c@example.com
Subject: continuations
Content-Type: multipart/mixed; boundary="RC"

--RC
Content-Type: application/octet-stream
Content-Disposition: attachment;
 filename*0*=UTF-8''%E6%96%87;
 filename*1*=%E4%BB%B6;
 filename*2=".txt"

data
--RC
Content-Type: text/plain; charset*=us-ascii'en'utf-8

Charset via RFC 2231
--RC--
"""

CORPUS["invalid_utf8_body.eml"] = (
    b"From: i@example.com\nSubject: invalid utf8\nContent-Type: text/plain; charset=utf-8\n\n"
    b"good \xff\xfe bad \xc3\xa9 end\n"
)

CORPUS["bad_b_word.eml"] = b"""From: x@example.com
Subject: =?utf-8?b?abcde?=

Body
"""

CORPUS["unknown_word_charset.eml"] = b"""From: x@example.com
Subject: =?x-foo?q?abc?=

Body
"""

CORPUS["custom_raw_utf8.eml"] = (
    "From: x@example.com\nSubject: ok\nX-Note: naïve\n\nBody\n"
).encode("utf-8")

CORPUS["bcc_cc.eml"] = b"""From: x@example.com
To:
Cc: a@example.com,  b@example.com ,
Bcc: hidden@example.com
Subject:
In-Reply-To: <parent@example.com>
References: <root@example.com> <parent@example.com>

Body
"""

CORPUS["qp_soft_breaks.eml"] = (
    b"From: q@example.com\nSubject: qp\nContent-Type: text/plain; charset=utf-8\n"
    b"Content-Transfer-Encoding: quoted-printable\n\n"
    b"a=3Db=\nc=\r\nd=4 =ZZ =\rx\nend==\n"
)

CORPUS["crlf_multipart.eml"] = (
    b"From: crlf@example.com\r\nSubject: CRLF multipart\r\n"
    b"Content-Type: multipart/alternative; boundary=\"CR\"\r\n\r\n"
    b"--CR\r\nContent-Type: text/plain\r\n\r\nplain\r\nline\r\n"
    b"--CR\r\nContent-Type: text/html\r\n\r\n<p>html</p>\r\n"
    b"--CR--\r\n"
)

CORPUS["cr_only.eml"] = (
    b"From: cr@example.com\rSubject: CR only\r\rbody one\rbody two\r"
)

CORPUS["header_colon_first.eml"] = b"""From: c@example.com
: no name
 continuation of nothing
Subject: after defects

Body
"""

CORPUS["misplaced_from.eml"] = b"""Subject: misplaced
From someone@example.com Mon Jan 15 10:00:00 2024
From: real@example.com

Body
"""

CORPUS["space_before_colon.eml"] = b"""Subject : spaced name
From: s@example.com

Body
"""

CORPUS["boundary_edge.eml"] = (
    b"From: e@example.com\nSubject: boundary edges\n"
    b"Content-Type: multipart/mixed; boundary=plain-b\n"
    b"Content-Transfer-Encoding: base64\n\n"
    b"\n\n"
    b"--plain-b  \n"
    b"--plain-b\n"
    b"Content-Type: text/plain\n\nfirst\n"
    b"--plain-b\r\n"
    b"Content-Type: multipart/alternative; boundary=\"in\"\n\n"
    b"--in\nContent-Type: text/plain\n\ninner\n--in--\n"
    b"--plain-b\n"
    b"Content-Type: text/html\n\n<p>last</p>\n"
    b"--plain-b--"
)

CORPUS["params_edge.eml"] = b"""From: p@example.com
Subject: parameter edges
Content-Type: multipart/mixed; boundary="PE"

--PE
Content-Type: text/plain; charset="utf\\"-8"

quoted charset
--PE
Content-Type: application/octet-stream; name="a;b.txt"
Content-Disposition: attachment; filename="a;b.txt"

semicolon name
--PE
Content-Type: application/octet-stream
Content-Disposition: attachment; filename=" spaced.txt "

spaced
--PE
Content-Type: application/octet-stream
Content-Disposition: attachment; filename*=x-unknown''abc%41.txt

unknown charset 2231
--PE
Content-Type: application/octet-stream
Content-Disposition: attachment; filename*=''plain%20name.txt

empty charset 2231
--PE
Content-Type: application/octet-stream
Content-Disposition: attachment; filename*=utf-8''one; filename*0=two

mixed continuations
--PE
Content-Type: text
Content-Disposition: attachment; filename="=?x-nope?q?bad?="

bad word charset
--PE
Content-Type: text/html
Content-Disposition: attachment; filename="page.html"

<p>attached html</p>
--PE--
"""

CORPUS["cte_trailing_space.eml"] = b"""From: t@example.com
Subject: cte with trailing space
Content-Transfer-Encoding: base64 \nContent-Type: text/plain

aGVsbG8=
"""

CORPUS["attached_multipart.eml"] = b"""From: am@example.com
Subject: attached multipart
Content-Type: multipart/mixed; boundary="AM"

--AM
Content-Type: multipart/alternative; boundary="AMI"
Content-Disposition: attachment; filename="bundle"

--AMI
Content-Type: text/plain

inside attached multipart
--AMI--
--AM--
"""

CORPUS["root_rfc822.eml"] = b"""From: outer@example.com
Subject: root rfc822
Content-Type: message/rfc822

From: inner@example.com
Subject: inner subject

inner body
"""

CORPUS["empty_boundary.eml"] = b"""From: eb@example.com
Subject: empty boundary
Content-Type: multipart/mixed; boundary=""

--
Content-Type: text/plain

part
----
"""

CORPUS["date_comment.eml"] = b"""From: dc@example.com
Subject: date with comment
Date: Mon, 15 Jan 2024 10:00:00 +0000 (UTC)

Body
"""

CORPUS["last_line_from.eml"] = b"""Subject: from at end
From someone else
Body?
"""

CORPUS["mixed_line_endings.eml"] = (
    b"From: ml@example.com\r\nSubject: mixed\nContent-Type: multipart/mixed; boundary=\"ML\"\r\n\n"
    b"--ML\r\nContent-Type: text/plain\n\nline1\r\nline2\rline3\n--ML--\r\n"
)

CORPUS["unicode_whitespace_refs.eml"] = (
    "From: u@example.com\nSubject: refs\nReferences: <a@x>\u00a0<b@x>\u2003<c@x>\n\nBody\n"
).encode("utf-8")

for cs, text in (
    ("iso-8859-9", "\u015eeker \u0130stanbul \u011f\u00fc"),
    ("ks_c_5601-1987", "\uc548\ub155\ud558\uc138\uc694"),
    ("cp949", "\ud55c\uad6d\uc5b4"),
    ("shift_jis", "\u3053\u3093\u306b\u3061\u306f"),
    ("iso-2022-jp", "\u65e5\u672c\u8a9e"),
    ("euc-jp", "\u6771\u4eac"),
    ("gb2312", "\u4f60\u597d\u4e16\u754c"),
    ("big5", "\u81fa\u7063"),
    ("koi8-r", "\u041f\u0440\u0438\u0432\u0435\u0442"),
    ("windows-1251", "\u041c\u0438\u0440"),
    ("utf-16", "wide \u00e9"),
    ("tis-620", "\u0e2a\u0e27\u0e31\u0e2a\u0e14\u0e35"),
):
    CORPUS["charset_%s.eml" % cs] = (
        ("From: cs@example.com\nSubject: charset %s\nContent-Type: text/plain; charset=%s\n\n" % (cs, cs)).encode("ascii")
        + text.encode(cs)
    )

big_lines = []
size = 0
i = 0
while size < 7000:
    line = "line %05d %s\r\n" % (i, "x" * (i % 70))
    big_lines.append(line)
    size += len(line)
    i += 1
big_head = "From: big@example.com\r\nSubject: big\r\nContent-Type: multipart/mixed; boundary=\"BIG\"\r\n\r\n--BIG\r\nContent-Type: text/plain\r\n\r\n"
big_text = big_head + "".join(big_lines)
# Place a CRLF exactly across the 8192-character chunk boundary.
pad = 8191 - (len(big_text) % 8192)
big_text += "y" * pad + "\r\n" + "--BIG\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=big.bin\r\nContent-Transfer-Encoding: base64\r\n\r\n"
import base64 as _b64
big_text += _b64.encodebytes(bytes(range(256)) * 8).decode("ascii").replace("\n", "\r\n") + "--BIG--\r\n"
CORPUS["big_crlf.eml"] = big_text.encode("ascii")

for name, data in CORPUS.items():
    with open(os.path.join(OUT_DIR, name), "wb") as f:
        f.write(data)

# ---------------------------------------------------------------- EmailParser files
parser = EmailParser()
files = {}
for name in CORPUS:
    path = os.path.join(OUT_DIR, name)
    rec = {}
    for opts_name, opts in (("default", {}), ("no_attachments", {"extract_attachments": False})):
        try:
            rec[opts_name] = {"ok": email_data_json(parser.parse_email(path, **opts))}
        except Exception as e:  # noqa: BLE001
            rec[opts_name] = err(e)
    data = CORPUS[name]
    rec["structure_bytes"] = safe(lambda: describe(message_from_bytes(data)))
    files[name] = rec
OUT["files"] = files

# --------------------------------------------------------- EmailParser text / bytes
STRING_CASES = {
    "short_text": "From: a@b.com\nTo: c@d.com\nSubject: Hi there\n\nShort body\n",
    "unicode_body": "Subject: Test\nContent-Type: text/plain; charset=utf-8\n\ncafé ☕ ok\n",
    "unicode_header": "Subject: Grüße 你好\nX-Info: été\n\nBody\n",
    "unicode_encoded_mix": "Subject: =?utf-8?q?Hi?= 你好 é\n\nBody\n",
    "unicode_qp_body": "Subject: qp\nContent-Transfer-Encoding: quoted-printable\n\nx=3Dy é\n",
    "no_headers": "Just a body line without headers\n",
    "long_text": "From: long@example.com\nSubject: Long\n\n" + ("word " * 80) + "\n",
    "with_slashes": "Subject: a/b/c\n\nbody/with/slashes\n",
    "empty": "",
    "dot": ".",
    "crlf_text": "From: c@d.com\r\nSubject: CRLF text\r\n\r\nline\r\n",
    "nul_char": "Subject: has \x00 nul\n\nbody\n",
    "attachment_text": "Content-Type: multipart/mixed; boundary=X\n\n--X\nContent-Disposition: attachment; filename=f.txt\n\ndata \u00e9\n--X--\n",
}
strings = {}
for name, text in STRING_CASES.items():
    try:
        strings[name] = {"input": text, "result": {"ok": email_data_json(parser.parse_email(text))}}
    except Exception as e:  # noqa: BLE001
        strings[name] = {"input": text, "result": err(e)}
    try:
        strings[name]["structure"] = {"ok": describe(message_from_string(text))}
    except Exception as e:  # noqa: BLE001
        strings[name]["structure"] = err(e)
OUT["strings"] = strings

BYTES_CASES = {
    "bytes_simple": b"From: a@b.com\nSubject: Bytes\n\nBytes body \xc3\xa9\n",
    "bytes_raw_header": b"Subject: caf\xc3\xa9\n\nBody\n",
}
bytes_out = {}
for name, data in BYTES_CASES.items():
    try:
        bytes_out[name] = {"input": list(data), "result": {"ok": email_data_json(parser.parse_email(data))}}
    except Exception as e:  # noqa: BLE001
        bytes_out[name] = {"input": list(data), "result": err(e)}
OUT["bytes"] = bytes_out

from pathlib import Path  # noqa: E402



def placeholder(rec):
    if "message" in rec:
        rec["message"] = rec["message"].replace(os.path.abspath(OUT_DIR), "<DIR>")
    return rec


OUT["missing_path"] = placeholder(safe(lambda: email_data_json(parser.parse_email(Path(OUT_DIR) / "does-not-exist.eml"))))
OUT["directory_path"] = placeholder(safe(lambda: email_data_json(parser.parse_email(Path(OUT_DIR)))))

# ----------------------------------------------------------------------- threads
thread_inputs = ["simple.eml", "folded.eml", "rfc822.eml", "bcc_cc.eml", "dates.eml", "date_naive.eml"]
emails = [parser.parse_email(os.path.join(OUT_DIR, n)) for n in thread_inputs]
analysis = EmailThreadAnalyzer().analyze_thread(emails)
analysis["participants"] = sorted(analysis["participants"])
OUT["thread"] = {"inputs": thread_inputs, "analysis": jsonable(analysis)}
OUT["thread_empty"] = jsonable(parser.analyze_thread([]))

# ------------------------------------------------------------------ MIMEParser
mp = MIMEParser()
OUT["mime_parser"] = {
    "text": safe(lambda: describe(mp.parse_mime_message("Subject: m\nContent-Type: text/html\n\n<b>x</b>\n"))),
    "bytes": safe(lambda: describe(mp.parse_mime_message(b"Subject: m\n\nbody \xe9\n"))),
}

# --------------------------------------------------------------- decode_header
HEADERS = [
    "plain value",
    "  leading and trailing  ",
    "=?utf-8?q?Caf=C3=A9?=",
    "=?utf-8?q?a?= =?utf-8?q?b?=",
    "=?utf-8?q?a?=  \t =?iso-8859-1?q?b?=",
    "Hello =?utf-8?q?W=C3=B6rld?= and =?utf-8?b?5L2g5aW9?= end",
    "=?UTF-8?B?w6TDtsO8?=",
    "=?utf-8?b?w6TDtsO8?=\n =?utf-8?b?w6TDtsO8?=",
    "line one\n line two =?utf-8?q?x?=",
    "=?utf-8?b?abc?=",
    "=?utf-8?b?a?=",
    "=?utf-8?q?under_score?=",
    "=?utf8?q?x?=",
    "=?utf-8*en?q?lang?=",
    "=?utf-8?x?bad?=",
    "no =?encoded word",
    "é =?utf-8?q?x?= 你",
    "=?utf-8?q??=",
    "=?utf-8?b??=",
]
dh = []
for h in HEADERS:
    try:
        res = decode_header(h)
        dh.append({"input": h, "ok": [[{"b": list(p)} if isinstance(p, bytes) else {"s": p}, cs] for p, cs in res]})
    except Exception as e:  # noqa: BLE001
        dh.append({"input": h, **err(e)})
OUT["decode_header"] = dh

# ------------------------------------------------------------------------- dates
DATES = [
    "Mon, 15 Jan 2024 10:30:00 +0000",
    "Tue, 16 Jan 2024 08:00:00 -0500",
    "16 Jan 2024 08:00:00 +0530",
    "Wed, 17 Jan 24 12:00:00 GMT",
    "Thu, 18 Jan 99 09:15 EST",
    "Fri,19 Jan 2024 18:45:30 +0900",
    "19 Jan 2024 18:45:30",
    "Jan 19 2024 18:45:30 +0100",
    "1 Jan 2024 10:00:00 -0000",
    "1 Jan 2024 10.00.00 +0000",
    "1 Jan 2024 10.00 +0000",
    "Mon, 32 Jan 2024 10:00:00 +0000",
    "Mon, 15 Jan 2024 25:00:00 +0000",
    "Mon, 15 Jan 2024 10:00:00 +2400",
    "Mon, 15 Jan 2024 10:00:00 +0099",
    "Mon, 15 Jan 2024 10:00:00 PDT",
    "15-Jan-2024 10:00:00 +0000",
    "Monday, 15-Jan-24 10:00:00 GMT",
    "15 January 2024 10:00:00 +0100",
    "not a date",
    "",
    "   ",
    "Mon, 15 Jan 2024",
    "Mon, 15 Foo 2024 10:00:00 +0000",
    "15 Jan 2024 10:00:00+0200",
    "15 Jan 2024 10:00:00-0200",
    "15 Jan 2024, 10:00:00 +0000",
    "15 Jan 1_999 10:00:00 +0000",
    "29 Feb 2023 10:00:00 +0000",
    "29 Feb 2024 10:00:00 +0000",
    "15 Jan 0 10:00:00 +0000",
    "15 Jan 69 10:00:00 +0000",
    "15 Jan 2024 10:00:60 +0000",
    "15 Jan 2024 10:00:00 +12345",
]
pd = []
for s in DATES:
    try:
        pd.append({"input": s, "ok": parsedate_to_datetime(s).isoformat()})
    except Exception as e:  # noqa: BLE001
        pd.append({"input": s, **err(e)})
OUT["parsedate"] = pd

# ---------------------------------------------------------------------- binascii
B64 = [b"aGk=", b"aGk", b"aG k=", b"aGk==", b"aGk=x", b"a", b"ab", b"abc", b"abcd", b"=aGk", b"a=Gk",
       b"aGVsbG8gd29ybGQ", b"SGVsbG8g!!d29ybGQ", b"", b"====", b"QQ==QQ==", b"QQ=", b"QQ", b"Q", b"Q===",
       b"YWJj\nZGVm", b"YW Jj", b"YWJjZA=", b"YWJjZA", b"Y"]
b64 = []
for data in B64:
    row = {"input": list(data)}
    for strict in (False, True):
        try:
            row["strict" if strict else "lenient"] = {"ok": list(binascii.a2b_base64(data, strict_mode=strict))}
        except binascii.Error as e:
            row["strict" if strict else "lenient"] = err(e)
    row["decode_b"] = list(decode_b(data)[0])
    b64.append(row)
OUT["base64"] = b64

QP = [b"a=3Db", b"soft=\nbreak", b"soft=\r\nbreak", b"cr=\rx\ny", b"eq==x", b"bad=ZZ", b"end=", b"end=4",
      b"low=c3=a9", b"x_y", b"= \nz", b"=\n"]
OUT["qp"] = [{"input": list(d), "body": list(binascii.a2b_qp(d)), "header": list(binascii.a2b_qp(d, header=True))}
             for d in QP]

with open(os.path.join(OUT_DIR, "oracle.json"), "w") as f:
    json.dump(OUT, f, ensure_ascii=True, sort_keys=False, separators=(",", ":"))
print("wrote", len(CORPUS), "fixtures")
