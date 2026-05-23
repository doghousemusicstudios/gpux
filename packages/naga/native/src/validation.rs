use crate::results::{make_ok_validation, make_single_error, NagaValidationResult};

pub(crate) fn validate_module(
    module: &naga::Module,
    capabilities: naga::valid::Capabilities,
    source: &str,
) -> NagaValidationResult {
    match validate_module_info(module, capabilities, source) {
        Ok(_) => make_ok_validation(),
        Err(result) => result,
    }
}

pub(crate) fn validate_module_info(
    module: &naga::Module,
    capabilities: naga::valid::Capabilities,
    source: &str,
) -> Result<naga::valid::ModuleInfo, NagaValidationResult> {
    let mut validator =
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), capabilities);
    validator
        .subgroup_stages(naga::valid::ShaderStages::all())
        .subgroup_operations(naga::valid::SubgroupOperationSet::all());

    validator
        .validate(module)
        .map_err(|error| validation_error_to_result(&error, source))
}

fn validation_error_to_result(
    error: &naga::WithSpan<naga::valid::ValidationError>,
    source: &str,
) -> NagaValidationResult {
    let message = format_error_chain(error);
    let (offset, length) = error
        .spans()
        .next()
        .map(|(span, _)| {
            let loc = span.location(source);
            (loc.offset as i32, loc.length as i32)
        })
        .unwrap_or((-1, -1));

    make_single_error(&message, offset, length)
}

/// Format the full error chain by walking `source()`. naga's
/// validation errors carry the most informative root cause one or
/// more levels down (e.g. "Entry point main at Fragment is invalid"
/// → "Function [0] 'main' is invalid" → "Expression [12] is invalid"
/// → "Type resolution failed: 'sampler2D' is not a constructible
/// type"). The outermost message alone is rarely actionable.
fn format_error_chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut out = format!("{}", error);
    let mut current = error.source();
    while let Some(inner) = current {
        out.push_str(&format!("\n  Caused by: {}", inner));
        current = inner.source();
    }
    out
}
