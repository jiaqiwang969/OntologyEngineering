//! Characterisation tests for `excel_parser` (ExcelParser, ExcelSheet, ExcelData) with the
//! pandas `read_excel` and openpyxl paths, against the original (fixtures/office, section
//! "excel": sheets with mixed types, dates, NaNs, merged cells, header/skip/max options,
//! error paths).

mod office_support;

use office_support::{cases, check, file, options, to_json};
use sem_parse::excel_parser::ExcelResult;
use sem_parse::{ExcelData, ExcelParser, ExcelSheet, Options};

#[test]
fn test_excel_cases_match_oracle() {
    sem_utils::get_progress_tracker().set_displays(Vec::new());
    let parser = ExcelParser::new(Options::new());
    let all = cases("excel");
    assert!(all.len() >= 90);
    for case in all {
        let path = file(case["file"].as_str().unwrap());
        let sheet = case["sheet_name"].as_str();
        let result = match case["call"].as_str().unwrap() {
            "parse" => parser.parse(&path, sheet, &options(&case)).map(to_json),
            "extract_sheet" => parser
                .extract_sheet(&path, sheet.unwrap_or(""), &options(&case))
                .map(to_json),
            other => panic!("unknown call {other}"),
        };
        check(&case, result);
    }
}

#[test]
fn test_excel_result_and_dataclasses() {
    sem_utils::get_progress_tracker().set_displays(Vec::new());
    let parser = ExcelParser::default();
    let all = parser.parse(file("data.xlsx"), None, &Options::new()).unwrap();
    let data: &ExcelData = all.as_data().expect("all sheets");
    assert!(data.sheet_names.len() > 1);
    assert!(all.as_sheet().is_none());
    let one = parser.parse(file("data.xlsx"), Some("People"), &Options::new()).unwrap();
    let sheet: &ExcelSheet = one.as_sheet().expect("one sheet");
    assert_eq!(sheet.name, "People");
    assert!(matches!(one, ExcelResult::Sheet(_)));
    assert_eq!(to_json(sheet)["name"], "People");
    assert_eq!(parser.logger.name(), "semantica.excel_parser");
}
