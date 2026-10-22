//! Python standard-library and third-party behaviour the ingest port relies on, ported so the
//! original's observable output is kept (`urllib.parse`, `ipaddress`, `requests`, `html.parser`
//! + BeautifulSoup, `xml.etree`, `mimetypes`, `urllib.robotparser`, ...). These modules are new
//! infrastructure with no counterpart file in `semantica/ingest`.

pub mod aws;
pub mod compress;
pub mod dateparse;
pub mod etree;
pub mod hooks;
pub mod html;
pub mod http;
pub mod ipaddr;
pub mod kw;
pub mod mimetypes;
pub mod optional;
pub mod pycodec;
pub mod pycsv;
pub mod pyjson;
pub mod pypath;
pub mod pytime;
pub mod pyurl;
pub mod robotparser;
pub mod xmldom;
