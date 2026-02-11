#![no_main]

use libfuzzer_sys::fuzz_target;
use streamhouse_sql::parse_query;

fuzz_target!(|data: &[u8]| {
    // Feed arbitrary bytes as SQL strings to the parser.
    // Tests handling of:
    // - Malformed SQL syntax
    // - SQL injection patterns
    // - Extremely long strings
    // - Invalid UTF-8 (converted to lossy string)
    // - Special characters
    // - Deeply nested expressions
    // - Invalid identifiers
    let sql = String::from_utf8_lossy(data);

    // The parser should never panic — only return Ok or Err
    let _ = parse_query(&sql);
});
