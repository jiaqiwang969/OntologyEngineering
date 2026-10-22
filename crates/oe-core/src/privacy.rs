//! Public-candidate direct-identifier scan. This is not semantic authority or a
//! copyright/forensic audit. Git selection failures never become empty success.
use crate::process::{CommandSpec, run_command};
use crate::{
    SkillRoot, create_new_regular_file, fingerprint_file, open_regular_file, sha256, write_new,
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags, config::DbConfig, limits::Limit};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

const LINE_CHARS: usize = 16 * 1024 * 1024;
const SQL_TEXT_CHARS: usize = 1024 * 1024;
const BINARY: &[&str] = &[
    "7z", "avi", "doc", "docx", "gif", "gz", "heic", "jpeg", "jpg", "mov", "mp3", "mp4", "pdf",
    "png", "ppt", "pptx", "tar", "webp", "whl", "xls", "xlsx", "xz", "zip",
];
const FILENAMES: &[&str] = &[
    ".env",
    ".env.local",
    ".env.production",
    ".netrc",
    ".npmrc",
    ".pypirc",
    "id_ed25519",
    "id_rsa",
];
const PARTS: &[&str] = &["private", "sessions", "rollouts", "__pycache__"];
const FRAGMENTS: &[&str] = &[
    "sources/raw",
    "structured/mineru",
    "reports/imagegen-events/raw",
    ".claude/projects",
    ".codex/sessions",
];
const SUFFIXES: &[&str] = &["key", "p12", "pem", "pfx", "pyc"];

#[derive(Clone, Debug)]
pub struct ScanOptions {
    pub git: PathBuf,
    /// Fresh ROOT/var evidence directory; only this generated subtree is excluded.
    pub output: PathBuf,
    pub tracked_only: bool,
    pub include_ignored: bool,
}
impl ScanOptions {
    pub fn new(git: PathBuf, output: PathBuf) -> Self {
        Self {
            git,
            output,
            tracked_only: false,
            include_ignored: false,
        }
    }
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Finding {
    pub path: String,
    pub line: Option<usize>,
    pub rule: String,
}
fn finding(path: &str, rule: &str, line: Option<usize>) -> Finding {
    Finding {
        path: path.into(),
        rule: rule.into(),
        line,
    }
}
fn control(c: char) -> bool {
    (c < ' ' && !matches!(c, '\n' | '\r' | '\t')) || c == '\u{7f}'
}
// Python 3.13 Unicode 15.1.0 str.isalnum: exact regex \w letters/numbers.
// Generated from the standard-library character database, not corpus content.
const PYTHON_ALNUM: &[(u32, u32)] = &[
    (0x30, 0x39),
    (0x41, 0x5a),
    (0x61, 0x7a),
    (0xaa, 0xaa),
    (0xb2, 0xb3),
    (0xb5, 0xb5),
    (0xb9, 0xba),
    (0xbc, 0xbe),
    (0xc0, 0xd6),
    (0xd8, 0xf6),
    (0xf8, 0x2c1),
    (0x2c6, 0x2d1),
    (0x2e0, 0x2e4),
    (0x2ec, 0x2ec),
    (0x2ee, 0x2ee),
    (0x370, 0x374),
    (0x376, 0x377),
    (0x37a, 0x37d),
    (0x37f, 0x37f),
    (0x386, 0x386),
    (0x388, 0x38a),
    (0x38c, 0x38c),
    (0x38e, 0x3a1),
    (0x3a3, 0x3f5),
    (0x3f7, 0x481),
    (0x48a, 0x52f),
    (0x531, 0x556),
    (0x559, 0x559),
    (0x560, 0x588),
    (0x5d0, 0x5ea),
    (0x5ef, 0x5f2),
    (0x620, 0x64a),
    (0x660, 0x669),
    (0x66e, 0x66f),
    (0x671, 0x6d3),
    (0x6d5, 0x6d5),
    (0x6e5, 0x6e6),
    (0x6ee, 0x6fc),
    (0x6ff, 0x6ff),
    (0x710, 0x710),
    (0x712, 0x72f),
    (0x74d, 0x7a5),
    (0x7b1, 0x7b1),
    (0x7c0, 0x7ea),
    (0x7f4, 0x7f5),
    (0x7fa, 0x7fa),
    (0x800, 0x815),
    (0x81a, 0x81a),
    (0x824, 0x824),
    (0x828, 0x828),
    (0x840, 0x858),
    (0x860, 0x86a),
    (0x870, 0x887),
    (0x889, 0x88e),
    (0x8a0, 0x8c9),
    (0x904, 0x939),
    (0x93d, 0x93d),
    (0x950, 0x950),
    (0x958, 0x961),
    (0x966, 0x96f),
    (0x971, 0x980),
    (0x985, 0x98c),
    (0x98f, 0x990),
    (0x993, 0x9a8),
    (0x9aa, 0x9b0),
    (0x9b2, 0x9b2),
    (0x9b6, 0x9b9),
    (0x9bd, 0x9bd),
    (0x9ce, 0x9ce),
    (0x9dc, 0x9dd),
    (0x9df, 0x9e1),
    (0x9e6, 0x9f1),
    (0x9f4, 0x9f9),
    (0x9fc, 0x9fc),
    (0xa05, 0xa0a),
    (0xa0f, 0xa10),
    (0xa13, 0xa28),
    (0xa2a, 0xa30),
    (0xa32, 0xa33),
    (0xa35, 0xa36),
    (0xa38, 0xa39),
    (0xa59, 0xa5c),
    (0xa5e, 0xa5e),
    (0xa66, 0xa6f),
    (0xa72, 0xa74),
    (0xa85, 0xa8d),
    (0xa8f, 0xa91),
    (0xa93, 0xaa8),
    (0xaaa, 0xab0),
    (0xab2, 0xab3),
    (0xab5, 0xab9),
    (0xabd, 0xabd),
    (0xad0, 0xad0),
    (0xae0, 0xae1),
    (0xae6, 0xaef),
    (0xaf9, 0xaf9),
    (0xb05, 0xb0c),
    (0xb0f, 0xb10),
    (0xb13, 0xb28),
    (0xb2a, 0xb30),
    (0xb32, 0xb33),
    (0xb35, 0xb39),
    (0xb3d, 0xb3d),
    (0xb5c, 0xb5d),
    (0xb5f, 0xb61),
    (0xb66, 0xb6f),
    (0xb71, 0xb77),
    (0xb83, 0xb83),
    (0xb85, 0xb8a),
    (0xb8e, 0xb90),
    (0xb92, 0xb95),
    (0xb99, 0xb9a),
    (0xb9c, 0xb9c),
    (0xb9e, 0xb9f),
    (0xba3, 0xba4),
    (0xba8, 0xbaa),
    (0xbae, 0xbb9),
    (0xbd0, 0xbd0),
    (0xbe6, 0xbf2),
    (0xc05, 0xc0c),
    (0xc0e, 0xc10),
    (0xc12, 0xc28),
    (0xc2a, 0xc39),
    (0xc3d, 0xc3d),
    (0xc58, 0xc5a),
    (0xc5d, 0xc5d),
    (0xc60, 0xc61),
    (0xc66, 0xc6f),
    (0xc78, 0xc7e),
    (0xc80, 0xc80),
    (0xc85, 0xc8c),
    (0xc8e, 0xc90),
    (0xc92, 0xca8),
    (0xcaa, 0xcb3),
    (0xcb5, 0xcb9),
    (0xcbd, 0xcbd),
    (0xcdd, 0xcde),
    (0xce0, 0xce1),
    (0xce6, 0xcef),
    (0xcf1, 0xcf2),
    (0xd04, 0xd0c),
    (0xd0e, 0xd10),
    (0xd12, 0xd3a),
    (0xd3d, 0xd3d),
    (0xd4e, 0xd4e),
    (0xd54, 0xd56),
    (0xd58, 0xd61),
    (0xd66, 0xd78),
    (0xd7a, 0xd7f),
    (0xd85, 0xd96),
    (0xd9a, 0xdb1),
    (0xdb3, 0xdbb),
    (0xdbd, 0xdbd),
    (0xdc0, 0xdc6),
    (0xde6, 0xdef),
    (0xe01, 0xe30),
    (0xe32, 0xe33),
    (0xe40, 0xe46),
    (0xe50, 0xe59),
    (0xe81, 0xe82),
    (0xe84, 0xe84),
    (0xe86, 0xe8a),
    (0xe8c, 0xea3),
    (0xea5, 0xea5),
    (0xea7, 0xeb0),
    (0xeb2, 0xeb3),
    (0xebd, 0xebd),
    (0xec0, 0xec4),
    (0xec6, 0xec6),
    (0xed0, 0xed9),
    (0xedc, 0xedf),
    (0xf00, 0xf00),
    (0xf20, 0xf33),
    (0xf40, 0xf47),
    (0xf49, 0xf6c),
    (0xf88, 0xf8c),
    (0x1000, 0x102a),
    (0x103f, 0x1049),
    (0x1050, 0x1055),
    (0x105a, 0x105d),
    (0x1061, 0x1061),
    (0x1065, 0x1066),
    (0x106e, 0x1070),
    (0x1075, 0x1081),
    (0x108e, 0x108e),
    (0x1090, 0x1099),
    (0x10a0, 0x10c5),
    (0x10c7, 0x10c7),
    (0x10cd, 0x10cd),
    (0x10d0, 0x10fa),
    (0x10fc, 0x1248),
    (0x124a, 0x124d),
    (0x1250, 0x1256),
    (0x1258, 0x1258),
    (0x125a, 0x125d),
    (0x1260, 0x1288),
    (0x128a, 0x128d),
    (0x1290, 0x12b0),
    (0x12b2, 0x12b5),
    (0x12b8, 0x12be),
    (0x12c0, 0x12c0),
    (0x12c2, 0x12c5),
    (0x12c8, 0x12d6),
    (0x12d8, 0x1310),
    (0x1312, 0x1315),
    (0x1318, 0x135a),
    (0x1369, 0x137c),
    (0x1380, 0x138f),
    (0x13a0, 0x13f5),
    (0x13f8, 0x13fd),
    (0x1401, 0x166c),
    (0x166f, 0x167f),
    (0x1681, 0x169a),
    (0x16a0, 0x16ea),
    (0x16ee, 0x16f8),
    (0x1700, 0x1711),
    (0x171f, 0x1731),
    (0x1740, 0x1751),
    (0x1760, 0x176c),
    (0x176e, 0x1770),
    (0x1780, 0x17b3),
    (0x17d7, 0x17d7),
    (0x17dc, 0x17dc),
    (0x17e0, 0x17e9),
    (0x17f0, 0x17f9),
    (0x1810, 0x1819),
    (0x1820, 0x1878),
    (0x1880, 0x1884),
    (0x1887, 0x18a8),
    (0x18aa, 0x18aa),
    (0x18b0, 0x18f5),
    (0x1900, 0x191e),
    (0x1946, 0x196d),
    (0x1970, 0x1974),
    (0x1980, 0x19ab),
    (0x19b0, 0x19c9),
    (0x19d0, 0x19da),
    (0x1a00, 0x1a16),
    (0x1a20, 0x1a54),
    (0x1a80, 0x1a89),
    (0x1a90, 0x1a99),
    (0x1aa7, 0x1aa7),
    (0x1b05, 0x1b33),
    (0x1b45, 0x1b4c),
    (0x1b50, 0x1b59),
    (0x1b83, 0x1ba0),
    (0x1bae, 0x1be5),
    (0x1c00, 0x1c23),
    (0x1c40, 0x1c49),
    (0x1c4d, 0x1c7d),
    (0x1c80, 0x1c88),
    (0x1c90, 0x1cba),
    (0x1cbd, 0x1cbf),
    (0x1ce9, 0x1cec),
    (0x1cee, 0x1cf3),
    (0x1cf5, 0x1cf6),
    (0x1cfa, 0x1cfa),
    (0x1d00, 0x1dbf),
    (0x1e00, 0x1f15),
    (0x1f18, 0x1f1d),
    (0x1f20, 0x1f45),
    (0x1f48, 0x1f4d),
    (0x1f50, 0x1f57),
    (0x1f59, 0x1f59),
    (0x1f5b, 0x1f5b),
    (0x1f5d, 0x1f5d),
    (0x1f5f, 0x1f7d),
    (0x1f80, 0x1fb4),
    (0x1fb6, 0x1fbc),
    (0x1fbe, 0x1fbe),
    (0x1fc2, 0x1fc4),
    (0x1fc6, 0x1fcc),
    (0x1fd0, 0x1fd3),
    (0x1fd6, 0x1fdb),
    (0x1fe0, 0x1fec),
    (0x1ff2, 0x1ff4),
    (0x1ff6, 0x1ffc),
    (0x2070, 0x2071),
    (0x2074, 0x2079),
    (0x207f, 0x2089),
    (0x2090, 0x209c),
    (0x2102, 0x2102),
    (0x2107, 0x2107),
    (0x210a, 0x2113),
    (0x2115, 0x2115),
    (0x2119, 0x211d),
    (0x2124, 0x2124),
    (0x2126, 0x2126),
    (0x2128, 0x2128),
    (0x212a, 0x212d),
    (0x212f, 0x2139),
    (0x213c, 0x213f),
    (0x2145, 0x2149),
    (0x214e, 0x214e),
    (0x2150, 0x2189),
    (0x2460, 0x249b),
    (0x24ea, 0x24ff),
    (0x2776, 0x2793),
    (0x2c00, 0x2ce4),
    (0x2ceb, 0x2cee),
    (0x2cf2, 0x2cf3),
    (0x2cfd, 0x2cfd),
    (0x2d00, 0x2d25),
    (0x2d27, 0x2d27),
    (0x2d2d, 0x2d2d),
    (0x2d30, 0x2d67),
    (0x2d6f, 0x2d6f),
    (0x2d80, 0x2d96),
    (0x2da0, 0x2da6),
    (0x2da8, 0x2dae),
    (0x2db0, 0x2db6),
    (0x2db8, 0x2dbe),
    (0x2dc0, 0x2dc6),
    (0x2dc8, 0x2dce),
    (0x2dd0, 0x2dd6),
    (0x2dd8, 0x2dde),
    (0x2e2f, 0x2e2f),
    (0x3005, 0x3007),
    (0x3021, 0x3029),
    (0x3031, 0x3035),
    (0x3038, 0x303c),
    (0x3041, 0x3096),
    (0x309d, 0x309f),
    (0x30a1, 0x30fa),
    (0x30fc, 0x30ff),
    (0x3105, 0x312f),
    (0x3131, 0x318e),
    (0x3192, 0x3195),
    (0x31a0, 0x31bf),
    (0x31f0, 0x31ff),
    (0x3220, 0x3229),
    (0x3248, 0x324f),
    (0x3251, 0x325f),
    (0x3280, 0x3289),
    (0x32b1, 0x32bf),
    (0x3400, 0x4dbf),
    (0x4e00, 0xa48c),
    (0xa4d0, 0xa4fd),
    (0xa500, 0xa60c),
    (0xa610, 0xa62b),
    (0xa640, 0xa66e),
    (0xa67f, 0xa69d),
    (0xa6a0, 0xa6ef),
    (0xa717, 0xa71f),
    (0xa722, 0xa788),
    (0xa78b, 0xa7ca),
    (0xa7d0, 0xa7d1),
    (0xa7d3, 0xa7d3),
    (0xa7d5, 0xa7d9),
    (0xa7f2, 0xa801),
    (0xa803, 0xa805),
    (0xa807, 0xa80a),
    (0xa80c, 0xa822),
    (0xa830, 0xa835),
    (0xa840, 0xa873),
    (0xa882, 0xa8b3),
    (0xa8d0, 0xa8d9),
    (0xa8f2, 0xa8f7),
    (0xa8fb, 0xa8fb),
    (0xa8fd, 0xa8fe),
    (0xa900, 0xa925),
    (0xa930, 0xa946),
    (0xa960, 0xa97c),
    (0xa984, 0xa9b2),
    (0xa9cf, 0xa9d9),
    (0xa9e0, 0xa9e4),
    (0xa9e6, 0xa9fe),
    (0xaa00, 0xaa28),
    (0xaa40, 0xaa42),
    (0xaa44, 0xaa4b),
    (0xaa50, 0xaa59),
    (0xaa60, 0xaa76),
    (0xaa7a, 0xaa7a),
    (0xaa7e, 0xaaaf),
    (0xaab1, 0xaab1),
    (0xaab5, 0xaab6),
    (0xaab9, 0xaabd),
    (0xaac0, 0xaac0),
    (0xaac2, 0xaac2),
    (0xaadb, 0xaadd),
    (0xaae0, 0xaaea),
    (0xaaf2, 0xaaf4),
    (0xab01, 0xab06),
    (0xab09, 0xab0e),
    (0xab11, 0xab16),
    (0xab20, 0xab26),
    (0xab28, 0xab2e),
    (0xab30, 0xab5a),
    (0xab5c, 0xab69),
    (0xab70, 0xabe2),
    (0xabf0, 0xabf9),
    (0xac00, 0xd7a3),
    (0xd7b0, 0xd7c6),
    (0xd7cb, 0xd7fb),
    (0xf900, 0xfa6d),
    (0xfa70, 0xfad9),
    (0xfb00, 0xfb06),
    (0xfb13, 0xfb17),
    (0xfb1d, 0xfb1d),
    (0xfb1f, 0xfb28),
    (0xfb2a, 0xfb36),
    (0xfb38, 0xfb3c),
    (0xfb3e, 0xfb3e),
    (0xfb40, 0xfb41),
    (0xfb43, 0xfb44),
    (0xfb46, 0xfbb1),
    (0xfbd3, 0xfd3d),
    (0xfd50, 0xfd8f),
    (0xfd92, 0xfdc7),
    (0xfdf0, 0xfdfb),
    (0xfe70, 0xfe74),
    (0xfe76, 0xfefc),
    (0xff10, 0xff19),
    (0xff21, 0xff3a),
    (0xff41, 0xff5a),
    (0xff66, 0xffbe),
    (0xffc2, 0xffc7),
    (0xffca, 0xffcf),
    (0xffd2, 0xffd7),
    (0xffda, 0xffdc),
    (0x10000, 0x1000b),
    (0x1000d, 0x10026),
    (0x10028, 0x1003a),
    (0x1003c, 0x1003d),
    (0x1003f, 0x1004d),
    (0x10050, 0x1005d),
    (0x10080, 0x100fa),
    (0x10107, 0x10133),
    (0x10140, 0x10178),
    (0x1018a, 0x1018b),
    (0x10280, 0x1029c),
    (0x102a0, 0x102d0),
    (0x102e1, 0x102fb),
    (0x10300, 0x10323),
    (0x1032d, 0x1034a),
    (0x10350, 0x10375),
    (0x10380, 0x1039d),
    (0x103a0, 0x103c3),
    (0x103c8, 0x103cf),
    (0x103d1, 0x103d5),
    (0x10400, 0x1049d),
    (0x104a0, 0x104a9),
    (0x104b0, 0x104d3),
    (0x104d8, 0x104fb),
    (0x10500, 0x10527),
    (0x10530, 0x10563),
    (0x10570, 0x1057a),
    (0x1057c, 0x1058a),
    (0x1058c, 0x10592),
    (0x10594, 0x10595),
    (0x10597, 0x105a1),
    (0x105a3, 0x105b1),
    (0x105b3, 0x105b9),
    (0x105bb, 0x105bc),
    (0x10600, 0x10736),
    (0x10740, 0x10755),
    (0x10760, 0x10767),
    (0x10780, 0x10785),
    (0x10787, 0x107b0),
    (0x107b2, 0x107ba),
    (0x10800, 0x10805),
    (0x10808, 0x10808),
    (0x1080a, 0x10835),
    (0x10837, 0x10838),
    (0x1083c, 0x1083c),
    (0x1083f, 0x10855),
    (0x10858, 0x10876),
    (0x10879, 0x1089e),
    (0x108a7, 0x108af),
    (0x108e0, 0x108f2),
    (0x108f4, 0x108f5),
    (0x108fb, 0x1091b),
    (0x10920, 0x10939),
    (0x10980, 0x109b7),
    (0x109bc, 0x109cf),
    (0x109d2, 0x10a00),
    (0x10a10, 0x10a13),
    (0x10a15, 0x10a17),
    (0x10a19, 0x10a35),
    (0x10a40, 0x10a48),
    (0x10a60, 0x10a7e),
    (0x10a80, 0x10a9f),
    (0x10ac0, 0x10ac7),
    (0x10ac9, 0x10ae4),
    (0x10aeb, 0x10aef),
    (0x10b00, 0x10b35),
    (0x10b40, 0x10b55),
    (0x10b58, 0x10b72),
    (0x10b78, 0x10b91),
    (0x10ba9, 0x10baf),
    (0x10c00, 0x10c48),
    (0x10c80, 0x10cb2),
    (0x10cc0, 0x10cf2),
    (0x10cfa, 0x10d23),
    (0x10d30, 0x10d39),
    (0x10e60, 0x10e7e),
    (0x10e80, 0x10ea9),
    (0x10eb0, 0x10eb1),
    (0x10f00, 0x10f27),
    (0x10f30, 0x10f45),
    (0x10f51, 0x10f54),
    (0x10f70, 0x10f81),
    (0x10fb0, 0x10fcb),
    (0x10fe0, 0x10ff6),
    (0x11003, 0x11037),
    (0x11052, 0x1106f),
    (0x11071, 0x11072),
    (0x11075, 0x11075),
    (0x11083, 0x110af),
    (0x110d0, 0x110e8),
    (0x110f0, 0x110f9),
    (0x11103, 0x11126),
    (0x11136, 0x1113f),
    (0x11144, 0x11144),
    (0x11147, 0x11147),
    (0x11150, 0x11172),
    (0x11176, 0x11176),
    (0x11183, 0x111b2),
    (0x111c1, 0x111c4),
    (0x111d0, 0x111da),
    (0x111dc, 0x111dc),
    (0x111e1, 0x111f4),
    (0x11200, 0x11211),
    (0x11213, 0x1122b),
    (0x1123f, 0x11240),
    (0x11280, 0x11286),
    (0x11288, 0x11288),
    (0x1128a, 0x1128d),
    (0x1128f, 0x1129d),
    (0x1129f, 0x112a8),
    (0x112b0, 0x112de),
    (0x112f0, 0x112f9),
    (0x11305, 0x1130c),
    (0x1130f, 0x11310),
    (0x11313, 0x11328),
    (0x1132a, 0x11330),
    (0x11332, 0x11333),
    (0x11335, 0x11339),
    (0x1133d, 0x1133d),
    (0x11350, 0x11350),
    (0x1135d, 0x11361),
    (0x11400, 0x11434),
    (0x11447, 0x1144a),
    (0x11450, 0x11459),
    (0x1145f, 0x11461),
    (0x11480, 0x114af),
    (0x114c4, 0x114c5),
    (0x114c7, 0x114c7),
    (0x114d0, 0x114d9),
    (0x11580, 0x115ae),
    (0x115d8, 0x115db),
    (0x11600, 0x1162f),
    (0x11644, 0x11644),
    (0x11650, 0x11659),
    (0x11680, 0x116aa),
    (0x116b8, 0x116b8),
    (0x116c0, 0x116c9),
    (0x11700, 0x1171a),
    (0x11730, 0x1173b),
    (0x11740, 0x11746),
    (0x11800, 0x1182b),
    (0x118a0, 0x118f2),
    (0x118ff, 0x11906),
    (0x11909, 0x11909),
    (0x1190c, 0x11913),
    (0x11915, 0x11916),
    (0x11918, 0x1192f),
    (0x1193f, 0x1193f),
    (0x11941, 0x11941),
    (0x11950, 0x11959),
    (0x119a0, 0x119a7),
    (0x119aa, 0x119d0),
    (0x119e1, 0x119e1),
    (0x119e3, 0x119e3),
    (0x11a00, 0x11a00),
    (0x11a0b, 0x11a32),
    (0x11a3a, 0x11a3a),
    (0x11a50, 0x11a50),
    (0x11a5c, 0x11a89),
    (0x11a9d, 0x11a9d),
    (0x11ab0, 0x11af8),
    (0x11c00, 0x11c08),
    (0x11c0a, 0x11c2e),
    (0x11c40, 0x11c40),
    (0x11c50, 0x11c6c),
    (0x11c72, 0x11c8f),
    (0x11d00, 0x11d06),
    (0x11d08, 0x11d09),
    (0x11d0b, 0x11d30),
    (0x11d46, 0x11d46),
    (0x11d50, 0x11d59),
    (0x11d60, 0x11d65),
    (0x11d67, 0x11d68),
    (0x11d6a, 0x11d89),
    (0x11d98, 0x11d98),
    (0x11da0, 0x11da9),
    (0x11ee0, 0x11ef2),
    (0x11f02, 0x11f02),
    (0x11f04, 0x11f10),
    (0x11f12, 0x11f33),
    (0x11f50, 0x11f59),
    (0x11fb0, 0x11fb0),
    (0x11fc0, 0x11fd4),
    (0x12000, 0x12399),
    (0x12400, 0x1246e),
    (0x12480, 0x12543),
    (0x12f90, 0x12ff0),
    (0x13000, 0x1342f),
    (0x13441, 0x13446),
    (0x14400, 0x14646),
    (0x16800, 0x16a38),
    (0x16a40, 0x16a5e),
    (0x16a60, 0x16a69),
    (0x16a70, 0x16abe),
    (0x16ac0, 0x16ac9),
    (0x16ad0, 0x16aed),
    (0x16b00, 0x16b2f),
    (0x16b40, 0x16b43),
    (0x16b50, 0x16b59),
    (0x16b5b, 0x16b61),
    (0x16b63, 0x16b77),
    (0x16b7d, 0x16b8f),
    (0x16e40, 0x16e96),
    (0x16f00, 0x16f4a),
    (0x16f50, 0x16f50),
    (0x16f93, 0x16f9f),
    (0x16fe0, 0x16fe1),
    (0x16fe3, 0x16fe3),
    (0x17000, 0x187f7),
    (0x18800, 0x18cd5),
    (0x18d00, 0x18d08),
    (0x1aff0, 0x1aff3),
    (0x1aff5, 0x1affb),
    (0x1affd, 0x1affe),
    (0x1b000, 0x1b122),
    (0x1b132, 0x1b132),
    (0x1b150, 0x1b152),
    (0x1b155, 0x1b155),
    (0x1b164, 0x1b167),
    (0x1b170, 0x1b2fb),
    (0x1bc00, 0x1bc6a),
    (0x1bc70, 0x1bc7c),
    (0x1bc80, 0x1bc88),
    (0x1bc90, 0x1bc99),
    (0x1d2c0, 0x1d2d3),
    (0x1d2e0, 0x1d2f3),
    (0x1d360, 0x1d378),
    (0x1d400, 0x1d454),
    (0x1d456, 0x1d49c),
    (0x1d49e, 0x1d49f),
    (0x1d4a2, 0x1d4a2),
    (0x1d4a5, 0x1d4a6),
    (0x1d4a9, 0x1d4ac),
    (0x1d4ae, 0x1d4b9),
    (0x1d4bb, 0x1d4bb),
    (0x1d4bd, 0x1d4c3),
    (0x1d4c5, 0x1d505),
    (0x1d507, 0x1d50a),
    (0x1d50d, 0x1d514),
    (0x1d516, 0x1d51c),
    (0x1d51e, 0x1d539),
    (0x1d53b, 0x1d53e),
    (0x1d540, 0x1d544),
    (0x1d546, 0x1d546),
    (0x1d54a, 0x1d550),
    (0x1d552, 0x1d6a5),
    (0x1d6a8, 0x1d6c0),
    (0x1d6c2, 0x1d6da),
    (0x1d6dc, 0x1d6fa),
    (0x1d6fc, 0x1d714),
    (0x1d716, 0x1d734),
    (0x1d736, 0x1d74e),
    (0x1d750, 0x1d76e),
    (0x1d770, 0x1d788),
    (0x1d78a, 0x1d7a8),
    (0x1d7aa, 0x1d7c2),
    (0x1d7c4, 0x1d7cb),
    (0x1d7ce, 0x1d7ff),
    (0x1df00, 0x1df1e),
    (0x1df25, 0x1df2a),
    (0x1e030, 0x1e06d),
    (0x1e100, 0x1e12c),
    (0x1e137, 0x1e13d),
    (0x1e140, 0x1e149),
    (0x1e14e, 0x1e14e),
    (0x1e290, 0x1e2ad),
    (0x1e2c0, 0x1e2eb),
    (0x1e2f0, 0x1e2f9),
    (0x1e4d0, 0x1e4eb),
    (0x1e4f0, 0x1e4f9),
    (0x1e7e0, 0x1e7e6),
    (0x1e7e8, 0x1e7eb),
    (0x1e7ed, 0x1e7ee),
    (0x1e7f0, 0x1e7fe),
    (0x1e800, 0x1e8c4),
    (0x1e8c7, 0x1e8cf),
    (0x1e900, 0x1e943),
    (0x1e94b, 0x1e94b),
    (0x1e950, 0x1e959),
    (0x1ec71, 0x1ecab),
    (0x1ecad, 0x1ecaf),
    (0x1ecb1, 0x1ecb4),
    (0x1ed01, 0x1ed2d),
    (0x1ed2f, 0x1ed3d),
    (0x1ee00, 0x1ee03),
    (0x1ee05, 0x1ee1f),
    (0x1ee21, 0x1ee22),
    (0x1ee24, 0x1ee24),
    (0x1ee27, 0x1ee27),
    (0x1ee29, 0x1ee32),
    (0x1ee34, 0x1ee37),
    (0x1ee39, 0x1ee39),
    (0x1ee3b, 0x1ee3b),
    (0x1ee42, 0x1ee42),
    (0x1ee47, 0x1ee47),
    (0x1ee49, 0x1ee49),
    (0x1ee4b, 0x1ee4b),
    (0x1ee4d, 0x1ee4f),
    (0x1ee51, 0x1ee52),
    (0x1ee54, 0x1ee54),
    (0x1ee57, 0x1ee57),
    (0x1ee59, 0x1ee59),
    (0x1ee5b, 0x1ee5b),
    (0x1ee5d, 0x1ee5d),
    (0x1ee5f, 0x1ee5f),
    (0x1ee61, 0x1ee62),
    (0x1ee64, 0x1ee64),
    (0x1ee67, 0x1ee6a),
    (0x1ee6c, 0x1ee72),
    (0x1ee74, 0x1ee77),
    (0x1ee79, 0x1ee7c),
    (0x1ee7e, 0x1ee7e),
    (0x1ee80, 0x1ee89),
    (0x1ee8b, 0x1ee9b),
    (0x1eea1, 0x1eea3),
    (0x1eea5, 0x1eea9),
    (0x1eeab, 0x1eebb),
    (0x1f100, 0x1f10c),
    (0x1fbf0, 0x1fbf9),
    (0x20000, 0x2a6df),
    (0x2a700, 0x2b739),
    (0x2b740, 0x2b81d),
    (0x2b820, 0x2cea1),
    (0x2ceb0, 0x2ebe0),
    (0x2ebf0, 0x2ee5d),
    (0x2f800, 0x2fa1d),
    (0x30000, 0x3134a),
    (0x31350, 0x323af),
];
fn word(c: char) -> bool {
    let n = c as u32;
    c == '_'
        || PYTHON_ALNUM
            .binary_search_by(|(a, b)| {
                if *b < n {
                    std::cmp::Ordering::Less
                } else if *a > n {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .is_ok()
}
fn boundary_before(text: &str, start: usize) -> bool {
    text[..start].chars().next_back().is_none_or(|c| !word(c))
}
fn boundary_after(text: &str, end: usize) -> bool {
    text[end..].chars().next().is_none_or(|c| !word(c))
}
fn token(text: &str, prefix: &str, minimum: usize, exact: bool, allowed: fn(u8) -> bool) -> bool {
    text.match_indices(prefix).any(|(start, _)| {
        if !boundary_before(text, start) {
            return false;
        }
        let offset = start + prefix.len();
        let length = text.as_bytes()[offset..]
            .iter()
            .take_while(|b| allowed(**b))
            .count();
        if length < minimum {
            return false;
        }
        if exact {
            return length == minimum && boundary_after(text, offset + length);
        }
        // Python regex backtracking may terminate a token before a trailing '-'.
        (minimum..=length).rev().any(|n| {
            boundary_after(text, offset + n) && word(text.as_bytes()[offset + n - 1] as char)
        })
    })
}
fn ascii_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}
fn token_char(b: u8) -> bool {
    ascii_word(b) || b == b'-'
}
fn alnum(b: u8) -> bool {
    b.is_ascii_alphanumeric()
}
fn aws(b: u8) -> bool {
    b.is_ascii_uppercase() || b.is_ascii_digit()
}
fn personal(text: &str, prefix: &str, sep: char) -> bool {
    text.match_indices(prefix).any(|(start, _)| {
        let tail = &text[start + prefix.len()..];
        for (count, c) in tail.chars().enumerate() {
            if c == sep {
                return count > 0;
            }
            if (c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
                || matches!(c, '`' | '\'' | '"' | '<' | '>')
            {
                return false;
            }
        }
        false
    })
}
fn text_rules(text: &str, path: &str, line: Option<usize>) -> Vec<Finding> {
    let mut rules = Vec::new();
    if text.chars().any(control) {
        rules.push("invalid control character in text candidate");
    }
    if personal(text, "/Users/", '/') {
        rules.push("macOS personal absolute path");
    }
    if personal(text, "/home/", '/') {
        rules.push("Linux personal absolute path");
    }
    if text.char_indices().any(|(i, c)| {
        c.is_ascii_alphabetic()
            && text[i + c.len_utf8()..].starts_with(":\\Users\\")
            && personal(&text[i..], &format!("{c}:\\Users\\"), '\\')
    }) {
        rules.push("Windows personal absolute path");
    }
    if text.match_indices("/var/folders/").any(|(i, _)| {
        text.as_bytes()
            .get(i + 13)
            .is_some_and(|b| b.is_ascii_alphanumeric() || b"_/.-".contains(b))
    }) {
        rules.push("macOS attachment cache path");
    }
    if ["", "RSA ", "EC ", "OPENSSH "]
        .iter()
        .any(|v| text.contains(&format!("-----BEGIN {v}PRIVATE KEY-----")))
    {
        rules.push("private key material");
    }
    if token(text, "sk-", 20, false, token_char) {
        rules.push("OpenAI-style secret token");
    }
    if ["ghp_", "gho_", "ghu_", "ghs_", "ghr_"]
        .iter()
        .any(|p| token(text, p, 20, false, alnum))
        || token(text, "github_pat_", 20, false, ascii_word)
    {
        rules.push("GitHub secret token");
    }
    if ["AKIA", "ASIA"]
        .iter()
        .any(|p| token(text, p, 16, true, aws))
    {
        rules.push("AWS access key");
    }
    if token(text, "apikey_", 32, false, token_char) {
        rules.push("Legacy API token");
    }
    if text
        .match_indices("codex-clipboard-")
        .any(|(i, _)| text.as_bytes().get(i + 16).is_some_and(|b| token_char(*b)))
    {
        rules.push("clipboard attachment identifier");
    }
    rules
        .into_iter()
        .map(|rule| finding(path, rule, line))
        .collect()
}

/// Inspect an explicitly prepared text projection using the existing scanner.
/// Findings identify rules only; this does not certify arbitrary private data.
pub fn inspect_text(text: &str) -> Vec<Finding> {
    text_rules(text, "prepared_text", None)
}
/// Match the original Path.as_posix relative-path representation. Only the
/// Windows native separator is translated; a Unix filename's literal backslash
/// remains part of that filename. This projection performs no filesystem I/O.
fn portable_path_text(path: &Path) -> Option<Cow<'_, str>> {
    let text = path.to_str()?;
    #[cfg(windows)]
    {
        Some(Cow::Owned(text.replace('\\', "/")))
    }
    #[cfg(not(windows))]
    {
        Some(Cow::Borrowed(text))
    }
}
fn display(path: &Path) -> String {
    let portable = portable_path_text(path);
    let sensitive = |s: &str| {
        text_rules(s, "", None)
            .iter()
            .any(|f| f.rule != "invalid control character in text candidate")
    };
    if path.to_str().is_some_and(sensitive) || portable.as_deref().is_some_and(sensitive) {
        return format!(
            "redacted-path-sha256:{}",
            sha256(path.as_os_str().as_encoded_bytes())
        );
    }
    match portable {
        Some(text) if text.chars().any(|c| c < ' ' || c == '\u{7f}') => text
            .chars()
            .flat_map(|c| {
                if c == '\n' {
                    "\\n".chars().collect::<Vec<_>>()
                } else if c == '\r' {
                    "\\r".chars().collect()
                } else if c == '\t' {
                    "\\t".chars().collect()
                } else if c < ' ' || c == '\u{7f}' {
                    format!("\\x{:02x}", c as u32).chars().collect()
                } else if c == '\\' {
                    "\\\\".chars().collect()
                } else if c > '\u{ffff}' {
                    format!("\\U{:08x}", c as u32).chars().collect()
                } else if c > '\u{ff}' {
                    format!("\\u{:04x}", c as u32).chars().collect()
                } else if !c.is_ascii() {
                    format!("\\x{:02x}", c as u32).chars().collect()
                } else {
                    vec![c]
                }
            })
            .collect(),
        Some(text) => text.into_owned(),
        None => format!("{:?}", path.as_os_str()),
    }
}
fn path_rules(path: &Path, relative: &Path) -> Vec<Finding> {
    let rel = display(relative);
    let mut out = Vec::new();
    if relative.to_str().is_none() {
        out.push(finding(&rel, "non-UTF-8 candidate path", None));
    }
    if relative
        .to_str()
        .is_some_and(|s| s.chars().any(|c| c < ' ' || c == '\u{7f}'))
    {
        out.push(finding(&rel, "control character in candidate path", None));
    }
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => out.push(finding(
            &rel,
            "symbolic link is not allowed in a public candidate",
            None,
        )),
        Ok(m) if !m.is_file() => out.push(finding(
            &rel,
            "special filesystem node is not allowed in a public candidate",
            None,
        )),
        Err(_) => out.push(finding(&rel, "unreadable candidate file", None)),
        _ => {}
    }
    let name = relative.file_name().and_then(OsStr::to_str).unwrap_or("");
    if FILENAMES.contains(&name) || (name.starts_with(".env.") && name != ".env.example") {
        out.push(finding(&rel, "credential/config filename", None));
    }
    if name == ".DS_Store" {
        out.push(finding(&rel, "OS metadata file", None));
    }
    let suffix = relative
        .extension()
        .and_then(OsStr::to_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    if SUFFIXES.contains(&suffix.as_str()) {
        out.push(finding(&rel, "secret or generated binary suffix", None));
    }
    if relative
        .components()
        .any(|p| PARTS.iter().any(|v| p.as_os_str() == *v))
    {
        out.push(finding(
            &rel,
            "private/session/generated path component",
            None,
        ));
    }
    if FRAGMENTS
        .iter()
        .any(|p| portable_path_text(relative).is_some_and(|raw| raw.contains(p)))
    {
        out.push(finding(&rel, "forbidden private evidence path", None));
    }
    out
}

struct Hashed<R> {
    inner: R,
    hash: Sha256,
    bytes: u64,
}
impl<R: Read> Read for Hashed<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.hash.update(&buf[..n]);
        self.bytes += n as u64;
        Ok(n)
    }
}
// Universal-newline streaming, retaining Python's character (not byte) bound.
fn line(reader: &mut impl BufRead, skip_lf: &mut bool) -> Result<Option<Vec<u8>>> {
    let mut result = Vec::new();
    loop {
        let buf = reader.fill_buf()?;
        if buf.is_empty() {
            return Ok(if result.is_empty() {
                None
            } else {
                Some(result)
            });
        }
        if *skip_lf {
            let skip = usize::from(buf[0] == b'\n');
            *skip_lf = false;
            if skip > 0 {
                reader.consume(skip);
                continue;
            }
        }
        let buf = reader.fill_buf()?;
        let end = buf.iter().position(|b| matches!(b, b'\r' | b'\n'));
        let n = end.unwrap_or(buf.len());
        let room = (LINE_CHARS * 4 + 4).saturating_sub(result.len());
        let copied = n.min(room);
        result.extend_from_slice(&buf[..copied]);
        if copied < n || result.len() >= LINE_CHARS * 4 + 4 {
            reader.consume(copied);
            return Ok(Some(result));
        }
        if let Some(end) = end {
            *skip_lf = buf[end] == b'\r';
            result.push(b'\n');
            reader.consume(end + 1);
            return Ok(Some(result));
        }
        reader.consume(n);
    }
}
fn text_content(reader: &mut impl BufRead, path: &str) -> Result<Vec<Finding>> {
    let mut result = Vec::new();
    let mut number = 0;
    let mut skip = false;
    while let Some(bytes) = line(reader, &mut skip)? {
        number += 1;
        let Ok(text) = std::str::from_utf8(&bytes) else {
            return Ok(vec![finding(path, "invalid UTF-8 text candidate", None)]);
        };
        if text.chars().count() > LINE_CHARS {
            return Ok(vec![finding(
                path,
                "text line exceeds bounded privacy scan",
                Some(number),
            )]);
        }
        if text.contains('\0') {
            return Ok(vec![finding(
                path,
                "NUL byte in declared text candidate",
                Some(number),
            )]);
        }
        result.extend(text_rules(text, path, Some(number)));
        ensure!(result.len() <= 100_000, "privacy_findings_limit");
    }
    Ok(result)
}
fn sidecars(path: &Path) -> bool {
    ["-wal", "-shm", "-journal"].iter().any(|suffix| {
        let mut p = path.as_os_str().to_owned();
        p.push(suffix);
        fs::symlink_metadata(Path::new(&p)).is_ok()
    })
}
fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}
fn node_identity(path: &Path) -> Result<Value> {
    crate::no_symlinks(path.parent().context("privacy_parent_missing")?)?;
    let m = fs::symlink_metadata(path)?;
    let link_sha256 = if m.file_type().is_symlink() {
        Some(sha256(fs::read_link(path)?.as_os_str().as_encoded_bytes()))
    } else {
        None
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(
            json!({"device":m.dev(),"inode":m.ino(),"mode":m.mode(),"bytes":m.len(),"mtime":m.mtime(),"mtime_nsec":m.mtime_nsec(),"link_sha256":link_sha256}),
        )
    }
    #[cfg(not(unix))]
    {
        Ok(json!({"bytes":m.len(),"symlink":m.file_type().is_symlink(),"link_sha256":link_sha256}))
    }
}
fn uri(path: &Path) -> Result<String> {
    let value = path.to_str().context("privacy_snapshot_path_not_utf8")?;
    let escaped: String = value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"/._-~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    Ok(format!("file:{escaped}?mode=ro&immutable=1"))
}
fn virtual_module(definition: &str) -> Option<String> {
    // Identify the actual module, not a quoted value or comment mentioning
    // USING fts5. This tokenizer never executes SQL or interprets its semantics.
    let raw = definition.as_bytes();
    let mut words = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        if raw[i..].starts_with(b"--") {
            while i < raw.len() && raw[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if raw[i..].starts_with(b"/*") {
            i += 2;
            while i < raw.len() && !raw[i..].starts_with(b"*/") {
                i += 1;
            }
            i = (i + 2).min(raw.len());
            continue;
        }
        if b"'\"`[".contains(&raw[i]) {
            let end = if raw[i] == b'[' { b']' } else { raw[i] };
            i += 1;
            while i < raw.len() {
                if raw[i] == end {
                    i += 1;
                    if raw.get(i) == Some(&end) {
                        i += 1;
                        continue;
                    }
                    break;
                }
                i += 1;
            }
            continue;
        }
        if raw[i].is_ascii_alphabetic() || raw[i] == b'_' {
            let start = i;
            i += 1;
            while i < raw.len() && (raw[i].is_ascii_alphanumeric() || raw[i] == b'_') {
                i += 1;
            }
            words.push(definition[start..i].to_ascii_lowercase());
        } else {
            i += 1;
        }
    }
    if words.get(..3) != Some(&["create".into(), "virtual".into(), "table".into()]) {
        return None;
    }
    words
        .iter()
        .position(|w| w == "using")
        .and_then(|i| words.get(i + 1))
        .cloned()
        .or(Some("unrecognized".into()))
}
fn sqlite_content(path: &Path, rel: &str, result: &mut Vec<Finding>) -> Result<()> {
    let db = Connection::open_with_flags(
        uri(path)?,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    db.set_limit(Limit::SQLITE_LIMIT_LENGTH, 16 * 1024 * 1024);
    db.set_limit(Limit::SQLITE_LIMIT_SQL_LENGTH, 1024 * 1024);
    db.set_limit(Limit::SQLITE_LIMIT_ATTACHED, 0);
    db.set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false)?;
    ensure!(
        unsafe { rusqlite::ffi::sqlite3_enable_load_extension(db.handle(), 0) }
            == rusqlite::ffi::SQLITE_OK,
        "privacy_sqlite_extension_disable"
    );
    let started = Instant::now();
    let mut calls = 0usize;
    db.progress_handler(
        10_000,
        Some(move || {
            calls += 1;
            calls > 10_000 || started.elapsed() > Duration::from_secs(30)
        }),
    );
    db.execute_batch("PRAGMA query_only=ON; PRAGMA trusted_schema=OFF; PRAGMA temp_store=MEMORY; PRAGMA mmap_size=0;")?;
    if db.query_row::<i64, _, _>("PRAGMA freelist_count", [], |r| r.get(0))? > 0 {
        result.push(finding(
            rel,
            "SQLite free pages require a vacuumed distribution copy",
            None,
        ));
    }
    let mut schema = db.prepare("SELECT type,name,sql FROM sqlite_schema ORDER BY name")?;
    let mut rows = schema.query([])?;
    let mut seen = 0;
    while let Some(row) = rows.next()? {
        seen += 1;
        ensure!(seen <= 4096, "privacy_sqlite_schema_limit");
        let kind: String = row.get(0)?;
        let name: String = row.get(1)?;
        let definition: Option<String> = row.get(2)?;
        result.extend(text_rules(&name, rel, None));
        if let Some(d) = &definition {
            result.extend(text_rules(d, rel, None));
        }
        if kind != "table" {
            continue;
        }
        if definition
            .as_ref()
            .and_then(|d| virtual_module(d))
            .is_some_and(|module| module != "fts5")
        {
            result.push(finding(
                rel,
                "unsupported SQLite virtual table for privacy scan",
                None,
            ));
            continue;
        }
        let mut columns = db.prepare(&format!("PRAGMA table_xinfo({})", quote(&name)))?;
        let names = columns
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for column in names {
            let col = quote(&column);
            // TEXT length/substr stop at embedded NUL in SQLite. Read bounded
            // UTF-8 bytes for declared TEXT so suffixes cannot hide identifiers.
            // Original BLOB values remain excluded by typeof, never decoded.
            let byte_limit = SQL_TEXT_CHARS * 4;
            let mut select=db.prepare(&format!("SELECT length(CAST({col} AS BLOB)),CASE WHEN length(CAST({col} AS BLOB))>{byte_limit} THEN NULL ELSE substr(CAST({col} AS BLOB),1,{byte_limit}) END FROM {} WHERE typeof({col})='text'",quote(&name)))?;
            let mut values = select.query([])?;
            while let Some(row) = values.next()? {
                let length: i64 = row.get(0)?;
                if length > byte_limit as i64 {
                    result.push(finding(
                        rel,
                        "SQLite text value exceeds bounded privacy scan",
                        None,
                    ));
                    continue;
                }
                let raw: Vec<u8> = row.get(1)?;
                let text = std::str::from_utf8(&raw)?;
                if text.chars().count() > SQL_TEXT_CHARS {
                    result.push(finding(
                        rel,
                        "SQLite text value exceeds bounded privacy scan",
                        None,
                    ));
                    continue;
                }
                result.extend(text_rules(text, rel, None));
                ensure!(result.len() <= 100_000, "privacy_findings_limit");
            }
        }
    }
    Ok(())
}

fn filesystem(root: &Path, output: &Path) -> Result<Vec<PathBuf>> {
    fn walk(root: &Path, dir: &Path, output: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path == output || entry.file_name() == ".git" {
                continue;
            }
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.is_dir() && !metadata.file_type().is_symlink() {
                walk(root, &path, output, out)?;
            } else {
                out.push(path.strip_prefix(root)?.to_owned());
            }
            ensure!(out.len() <= 1_000_000, "privacy_candidate_count_limit");
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(root, root, output, &mut out)?;
    out.sort();
    Ok(out)
}
fn candidates(
    root: &SkillRoot,
    options: &ScanOptions,
    output: &Path,
    stage: &str,
    processes: &mut Vec<Value>,
) -> Result<Vec<PathBuf>> {
    if options.include_ignored {
        return filesystem(root.path(), output);
    }
    let mut arguments: Vec<OsString> = ["-c", "core.fsmonitor=false", "ls-files", "-z"]
        .iter()
        .map(OsString::from)
        .collect();
    if !options.tracked_only {
        arguments.extend(
            ["--cached", "--others", "--exclude-standard"]
                .iter()
                .map(OsString::from),
        );
    }
    let evidence = output.join(stage);
    let observation = run_command(
        &CommandSpec {
            executable: options.git.clone(),
            arguments,
            cwd: root.path().to_owned(),
            environment: BTreeMap::new(),
            remove_environment: std::env::vars_os()
                .filter_map(|(k, _)| {
                    k.to_str()
                        .is_some_and(|v| v.starts_with("GIT_"))
                        .then_some(k)
                })
                .collect(),
            timeout: Duration::from_secs(30),
            output_limit: 64 * 1024 * 1024,
        },
        &evidence,
    )?;
    let report = evidence.join("process.json");
    processes.push(json!({"path":display(report.strip_prefix(root.path())?),"sha256":fingerprint_file(&report,u64::MAX)?.sha256,"status":observation.status,"exit_code":observation.exit_code}));
    ensure!(
        observation.status == "completed" && observation.exit_code == Some(0),
        "privacy_git_selection_failed"
    );
    ensure!(
        observation.stdout.is_empty() || observation.stdout.last() == Some(&0),
        "privacy_git_selection_malformed"
    );
    let mut out = BTreeSet::new();
    for name in observation
        .stdout
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
    {
        #[cfg(unix)]
        let rel = {
            use std::os::unix::ffi::OsStringExt;
            PathBuf::from(OsString::from_vec(name.to_vec()))
        };
        #[cfg(not(unix))]
        let rel = PathBuf::from(std::str::from_utf8(name)?);
        ensure!(
            !rel.is_absolute()
                && rel.components().all(|c| matches!(c, Component::Normal(_)))
                && !rel.components().any(|c| c.as_os_str() == ".git"),
            "privacy_git_path_invalid"
        );
        let path = root.path().join(&rel);
        if path.starts_with(output) {
            continue;
        }
        match fs::symlink_metadata(&path) {
            Ok(_) => {
                out.insert(rel);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => anyhow::bail!("privacy_candidate_metadata_unavailable"),
        }
    }
    Ok(out.into_iter().collect())
}
fn one(
    root: &SkillRoot,
    rel: &Path,
    output: &Path,
    ordinal: usize,
) -> Result<(Vec<Finding>, Value)> {
    let path = root.path().join(rel);
    let name = display(rel);
    let mut findings = path_rules(&path, rel);
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        let node = node_identity(&path)?;
        return Ok((
            findings,
            json!({"path":name,"kind":if metadata.file_type().is_symlink(){"symlink"}else{"special"},"bytes":metadata.len(),"node_identity":node,"content_scanned":false,"unchanged":true}),
        ));
    }
    let before = fingerprint_file(&path, u64::MAX)?;
    let suffix = rel
        .extension()
        .and_then(OsStr::to_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    let path_safe = rel
        .to_str()
        .is_some_and(|s| !s.chars().any(|c| c < ' ' || c == '\u{7f}'));
    let mut kind = "binary_exempt";
    if path_safe && !BINARY.contains(&suffix.as_str()) {
        let file = open_regular_file(&path, u64::MAX)?;
        let hashed = Hashed {
            inner: file,
            hash: Sha256::new(),
            bytes: 0,
        };
        let mut reader = BufReader::new(hashed);
        if ["sqlite", "sqlite3", "db"].contains(&suffix.as_str()) {
            kind = "sqlite_live_text";
            if sidecars(&path) {
                findings.push(finding(
                    &name,
                    "SQLite candidate must be standalone without sidecars",
                    None,
                ));
            } else {
                let snapshot = output.join(format!("sqlite-{ordinal}.db"));
                let mut target = create_new_regular_file(&snapshot)?;
                std::io::copy(&mut reader, &mut target)?;
                target.sync_all()?;
                drop(target);
                let mut header = [0; 16];
                let valid = open_regular_file(&snapshot, u64::MAX)?
                    .read_exact(&mut header)
                    .is_ok()
                    && &header == b"SQLite format 3\0";
                if !valid {
                    findings.push(finding(&name, "invalid SQLite candidate header", None));
                } else if sqlite_content(&snapshot, &name, &mut findings).is_err() {
                    findings.push(finding(
                        &name,
                        "unreadable or invalid SQLite candidate",
                        None,
                    ));
                }
                ensure!(
                    fingerprint_file(&snapshot, u64::MAX)? == before,
                    "privacy_sqlite_snapshot_changed"
                );
            }
        } else {
            kind = "utf8_text";
            match text_content(&mut reader, &name) {
                Ok(found) => findings.extend(found),
                Err(_) => findings.push(finding(&name, "unreadable candidate file", None)),
            }
        }
        std::io::copy(&mut reader, &mut std::io::sink())?;
        let observed = reader.into_inner();
        ensure!(
            observed.bytes == before.bytes
                && format!("{:x}", observed.hash.finalize()) == before.sha256,
            "privacy_scanned_bytes_changed"
        );
        if kind == "sqlite_live_text" && sidecars(&path) {
            findings.push(finding(
                &name,
                "SQLite candidate must be standalone without sidecars",
                None,
            ));
        }
    }
    ensure!(
        fingerprint_file(&path, u64::MAX)? == before,
        "privacy_candidate_changed"
    );
    if !path_safe {
        kind = "invalid_path_not_read";
    }
    Ok((
        findings,
        json!({"path":name,"kind":kind,"bytes":before.bytes,"sha256":before.sha256,"content_scanned":path_safe&&kind!="binary_exempt","unchanged":true}),
    ))
}
/// All input/runtime failures are explicit blocked reports with retained private
/// process receipts. Invalid scope/output arguments fail before creating output.
pub fn scan(root: &SkillRoot, options: &ScanOptions) -> Result<Value> {
    ensure!(
        !(options.tracked_only && options.include_ignored),
        "privacy_scope_conflict"
    );
    let output = root.new_private_dir(&options.output)?;
    let mut processes = Vec::new();
    let mut findings = Vec::new();
    let mut inventory = Vec::new();
    let mut selected = Vec::new();
    let mut unchanged = false;
    let mut selection_unchanged = false;
    let outcome = (|| -> Result<()> {
        selected = candidates(root, options, &output, "selection-before", &mut processes)?;
        for (ordinal, relative) in selected.iter().enumerate() {
            match one(root, relative, &output, ordinal) {
                Ok((found, identity)) => {
                    findings.extend(found);
                    inventory.push(identity);
                }
                Err(_) => {
                    findings.push(finding(
                        &display(relative),
                        "candidate unreadable or changed during privacy scan",
                        None,
                    ));
                    inventory.push(
                        json!({"path":display(relative),"kind":"unavailable","unchanged":false}),
                    );
                }
            }
            ensure!(findings.len() <= 100_000, "privacy_findings_limit");
        }
        let after = candidates(root, options, &output, "selection-after", &mut processes)?;
        ensure!(after == selected, "privacy_selection_changed");
        selection_unchanged = true;
        for (path, item) in selected.iter().zip(&mut inventory) {
            let verified = (|| -> Result<()> {
                if let Some(expected) = item.get("node_identity") {
                    ensure!(
                        &node_identity(&root.path().join(path))? == expected,
                        "privacy_candidate_node_changed"
                    );
                }
                if let Some(expected) = item["sha256"].as_str() {
                    let actual = fingerprint_file(&root.path().join(path), u64::MAX)?;
                    ensure!(
                        actual.sha256 == expected && Some(actual.bytes) == item["bytes"].as_u64(),
                        "privacy_candidate_changed"
                    );
                }
                if item["kind"] == "sqlite_live_text" {
                    ensure!(
                        !sidecars(&root.path().join(path)),
                        "privacy_sqlite_sidecar_appeared"
                    );
                }
                Ok(())
            })();
            if verified.is_err() {
                item["unchanged"] = false.into();
                verified?;
            }
        }
        unchanged = inventory.iter().all(|v| v["unchanged"] != false);
        Ok(())
    })();
    findings.sort();
    findings.dedup();
    let failed = outcome.is_err();
    let value = json!({"schema":"ontology-engineering.public-privacy-scan/v1","status":if failed||!findings.is_empty(){"blocked"}else{"completed"},"ok":!failed&&findings.is_empty(),"checked_files":inventory.len(),"selected_files":selected.len(),"findings":findings,"candidates":inventory,"selection_unchanged":selection_unchanged,"inputs_unchanged":unchanged,"effective_scope":if options.include_ignored{"full_tree_including_ignored"}else if options.tracked_only{"git_tracked_only"}else{"git_tracked_and_unignored"},"selection_reason":if options.include_ignored{"explicit_include_ignored"}else{"required_git_selection_no_fallback"},"excluded_generated_output":display(output.strip_prefix(root.path())?),"processes":processes,"error_code":if failed{Some("privacy_selection_or_resource_or_identity_failure")}else{None},"criteria_identity":{"implementation":"rust","source_sha256":sha256(include_bytes!("privacy.rs")),"compatibility":"check_public_privacy.py direct identifier rules","text_line_char_limit":LINE_CHARS,"sqlite_text_char_limit":SQL_TEXT_CHARS,"unicode_word_boundary_version":"15.1.0","sqlite_vm_instruction_limit":100_000_000,"sqlite_query_time_seconds":30,"sqlite_schema_entry_limit":4096,"sqlite_sql_length_byte_limit":1048576,"sqlite_value_byte_limit":16777216,"git_output_byte_limit":67108864,"git_timeout_seconds":30,"finding_limit":100000,"filesystem_candidate_limit":1000000,"binary_suffixes":BINARY},"semantic_authority":false,"publication":"not_performed"});
    write_new(&output.join("report.json"), &value)?;
    Ok(value)
}

#[cfg(test)]
mod path_admission_tests {
    use super::*;

    #[test]
    fn lexical_control_paths_are_rejected_and_escaped_without_a_file_body() {
        // Prospective/absent inputs exercise the same privacy path owner on
        // Windows as Unix. This is independent of OS filename creation.
        let missing = Path::new("absent-privacy-path-admission-no-file");
        assert!(!missing.exists());
        let found = path_rules(missing, Path::new("bad\nname.md"));
        assert!(
            found
                .iter()
                .any(|v| v.path == "bad\\nname.md"
                    && v.rule == "control character in candidate path")
        );
        assert!(found.iter().any(|v| v.rule == "unreadable candidate file"));
        assert!(!found.iter().any(|v| v.rule == "Legacy API token"));
        let secret = format!("{}{}", "apikey_", "7".repeat(48));
        let logical = format!("{secret}\nname.md");
        let redacted = path_rules(missing, Path::new(&logical));
        assert!(
            redacted
                .iter()
                .any(|v| v.rule == "control character in candidate path")
        );
        assert!(
            redacted
                .iter()
                .all(|v| v.path.starts_with("redacted-path-sha256:"))
        );
        assert!(!serde_json::to_string(&redacted).unwrap().contains(&secret));
    }

    #[test]
    fn fragment_projection_preserves_original_substrings_and_redacts_raw_identity() {
        let missing = Path::new("absent-privacy-fragment-no-file");
        assert!(!missing.exists());
        for text in [
            "sources/raw/book.bin",
            "structured/mineru/book.bin",
            "reports/imagegen-events/raw/book.bin",
            ".claude/projects/session.txt",
            ".codex/sessions/session.txt",
            "mysources/rawpost/book.bin",
        ] {
            let path = Path::new(text);
            let found = path_rules(missing, path);
            assert!(
                found
                    .iter()
                    .any(|v| v.rule == "forbidden private evidence path"),
                "{text}"
            );
            assert_eq!(display(path), text);
        }
        for text in ["public/sources-not-raw/book.bin", "SOURCES/raw/book.bin"] {
            assert!(
                !path_rules(missing, Path::new(text))
                    .iter()
                    .any(|v| v.rule == "forbidden private evidence path")
            );
        }
        let controlled = Path::new("sources/raw/bad\nname.bin");
        assert_eq!(display(controlled), "sources/raw/bad\\nname.bin");
        let secret = format!("{}{}", "apikey_", "7".repeat(48));
        let logical = PathBuf::from(format!("sources/raw/{secret}\nname.bin"));
        let expected = format!(
            "redacted-path-sha256:{}",
            sha256(logical.as_os_str().as_encoded_bytes())
        );
        let found = path_rules(missing, &logical);
        assert!(
            found
                .iter()
                .any(|v| v.rule == "forbidden private evidence path")
        );
        assert!(
            found
                .iter()
                .any(|v| v.rule == "control character in candidate path")
        );
        assert!(found.iter().all(|v| v.path == expected));
        assert!(!serde_json::to_string(&found).unwrap().contains(&secret));
    }

    #[cfg(unix)]
    #[test]
    fn unix_literal_backslash_and_non_utf8_names_are_not_reinterpreted() {
        use std::os::unix::ffi::OsStringExt;
        let missing = Path::new("absent-privacy-unix-name-no-file");
        assert!(!missing.exists());
        let literal = Path::new(r"sources\raw\book.bin");
        assert_eq!(display(literal), r"sources\raw\book.bin");
        assert!(
            !path_rules(missing, literal)
                .iter()
                .any(|v| v.rule == "forbidden private evidence path")
        );
        let non_utf8 = PathBuf::from(OsString::from_vec(b"sources/raw/\xff.bin".to_vec()));
        assert!(portable_path_text(&non_utf8).is_none());
        assert!(
            path_rules(missing, &non_utf8)
                .iter()
                .any(|v| v.rule == "non-UTF-8 candidate path")
        );
        assert_eq!(display(&non_utf8), format!("{:?}", non_utf8.as_os_str()));
    }

    #[cfg(windows)]
    #[test]
    fn windows_native_separators_project_without_lossy_surrogate_decoding() {
        use std::os::windows::ffi::OsStringExt;
        let missing = Path::new("absent-privacy-windows-name-no-file");
        assert!(!missing.exists());
        for (native, expected) in [
            (r"sources\raw\book.bin", "sources/raw/book.bin"),
            (r"structured\mineru\book.bin", "structured/mineru/book.bin"),
            (
                r"reports\imagegen-events\raw\book.bin",
                "reports/imagegen-events/raw/book.bin",
            ),
            (
                r".claude\projects\session.txt",
                ".claude/projects/session.txt",
            ),
            (
                r".codex\sessions\session.txt",
                ".codex/sessions/session.txt",
            ),
        ] {
            let path = Path::new(native);
            assert_eq!(display(path), expected);
            assert!(
                path_rules(missing, path)
                    .iter()
                    .any(|v| v.path == expected && v.rule == "forbidden private evidence path")
            );
        }
        let secret = format!("{}{}", "apikey_", "7".repeat(48));
        let native_secret = PathBuf::from(format!("sources\\raw\\{secret}\nname.bin"));
        let expected_raw_digest = sha256(native_secret.as_os_str().as_encoded_bytes());
        let projected = portable_path_text(&native_secret).unwrap();
        assert_ne!(expected_raw_digest, sha256(projected.as_bytes()));
        assert_eq!(
            display(&native_secret),
            format!("redacted-path-sha256:{expected_raw_digest}")
        );
        let protected = path_rules(missing, &native_secret);
        assert!(
            protected
                .iter()
                .any(|v| v.rule == "forbidden private evidence path")
        );
        assert!(!serde_json::to_string(&protected).unwrap().contains(&secret));

        // A legal native relative path can acquire an existing sensitive
        // pattern in the displayed representation. Check both representations,
        // while retaining the original raw-byte digest and raw Windows rules.
        let render_sensitive = Path::new(r"examples\Users\fixture\file.bin");
        assert!(text_rules(render_sensitive.to_str().unwrap(), "", None).is_empty());
        let shown = portable_path_text(render_sensitive).unwrap();
        assert!(
            text_rules(&shown, "", None)
                .iter()
                .any(|v| v.rule == "macOS personal absolute path")
        );
        assert_eq!(
            display(render_sensitive),
            format!(
                "redacted-path-sha256:{}",
                sha256(render_sensitive.as_os_str().as_encoded_bytes())
            )
        );
        let raw_sensitive = PathBuf::from(format!(r"C:\{}\{}\file.bin", "Users", "fixture"));
        assert!(
            text_rules(raw_sensitive.to_str().unwrap(), "", None)
                .iter()
                .any(|v| v.rule == "Windows personal absolute path")
        );
        assert_eq!(
            display(&raw_sensitive),
            format!(
                "redacted-path-sha256:{}",
                sha256(raw_sensitive.as_os_str().as_encoded_bytes())
            )
        );
        let control_only = Path::new("examples\\safe\\bad\nname.bin");
        assert_eq!(display(control_only), "examples/safe/bad\\nname.bin");
        let mut wide: Vec<_> = "sources\\raw\\".encode_utf16().collect();
        wide.push(0xd800);
        let non_utf8 = PathBuf::from(OsString::from_wide(&wide));
        assert!(portable_path_text(&non_utf8).is_none());
        assert!(
            path_rules(missing, &non_utf8)
                .iter()
                .any(|v| v.rule == "non-UTF-8 candidate path")
        );
        assert_eq!(display(&non_utf8), format!("{:?}", non_utf8.as_os_str()));
    }
}
