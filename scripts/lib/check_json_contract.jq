def schema_version_is_pos_int:
  (.schema_version | type == "number")
  and (.schema_version == (.schema_version | floor))
  and (.schema_version > 0);

def schema_in_range($min; $max):
  schema_version_is_pos_int
  and (.schema_version >= $min)
  and (.schema_version <= $max);

def summary_base:
  (.summary | type == "object")
  and (.summary.phase | type == "string")
  and (.summary.errors | type == "number")
  and (.summary.warnings | type == "number")
  and (.summary.counts | type == "object")
  and (.summary.counts.diagnostics | type == "number")
  and (.summary.counts.diagnostics == (.summary.errors + .summary.warnings));

def check_contract($expected_ok; $min_schema; $max_schema):
  (.ok == $expected_ok)
  and schema_in_range($min_schema; $max_schema)
  and summary_base
  and (.summary.phase == "check")
  and (.diagnostics | type == "array")
  and (([.diagnostics[]? | select(.severity == "error")] | length) == .summary.errors)
  and (([.diagnostics[]? | select(.severity == "warning")] | length) == .summary.warnings);

def modules_contract($expected_ok; $min_schema; $max_schema):
  (.ok == $expected_ok)
  and schema_in_range($min_schema; $max_schema)
  and summary_base
  and (.summary.phase == "check-modules")
  and (.modules | type == "array")
  and (([.modules[]?.diagnostics[]? | select(.severity == "error")] | length) == .summary.errors)
  and (([.modules[]?.diagnostics[]? | select(.severity == "warning")] | length) == .summary.warnings);

def load_contract($min_schema; $max_schema):
  (.ok == false)
  and schema_in_range($min_schema; $max_schema)
  and (.phase == "load")
  and summary_base
  and (.summary.phase == "load")
  and (.errors | type == "array")
  and (([.errors[]? | select(.severity == "error")] | length) == .summary.errors)
  and (([.errors[]? | select(.severity == "warning")] | length) == .summary.warnings);

def fixture_check_shape:
  schema_version_is_pos_int
  and summary_base
  and (.summary.phase == "check")
  and (.diagnostics | type == "array");

def fixture_modules_shape:
  schema_version_is_pos_int
  and summary_base
  and (.summary.phase == "check-modules")
  and (.modules | type == "array");

def fixture_load_shape:
  schema_version_is_pos_int
  and summary_base
  and (.phase == "load")
  and (.summary.phase == "load")
  and (.errors | type == "array");
