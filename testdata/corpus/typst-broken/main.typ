#set page(paper: "a4")

// Deliberately broken document for diagnostics testing:
// 1. Unknown function call
#undefined_custom_function_call("test")

// 2. Missing include file
#include "missing_chapter_file.typ"

// 3. Missing closing brace (causes parse-level syntax diagnostic)
#let calculate(val) = { val * 2
