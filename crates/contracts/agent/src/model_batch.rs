//! Canonical whole-batch grammar shared by admission and recovery.

use crate::{AgentFailure, ModelStep};

/// Maximum steps in a fresh or durably restored model batch.
const MAX_BATCH_STEPS: usize = 16;

/// Maximum tool calls in a fresh or durably restored tool batch.
const MAX_BATCH_TOOL_CALLS: usize = 8;

/// Whole-batch grammar, checked before any per-step validation or dispatch.
/// An answer batch is exactly one final answer with no tool or delegation;
/// a tool batch carries no answer or delegation; a delegation batch carries
/// exactly one delegation and nothing else executable. Every preamble leads
/// its batch: no preamble may follow an executable step.
pub fn validate_model_batch_shape(steps: &[ModelStep]) -> Result<(), AgentFailure> {
    if steps.is_empty() || steps.len() > MAX_BATCH_STEPS {
        return Err(AgentFailure::InvalidModelOutput);
    }
    if let Some(first_executable) = steps
        .iter()
        .position(|step| !matches!(step, ModelStep::Preamble { .. }))
    {
        if steps[first_executable..]
            .iter()
            .any(|step| matches!(step, ModelStep::Preamble { .. }))
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
    } else {
        return Err(AgentFailure::InvalidModelOutput);
    }
    let mut answers = 0;
    let mut tools = 0;
    let mut delegations = 0;
    for step in steps {
        match step {
            ModelStep::Answer { .. } => answers += 1,
            ModelStep::CallTool { .. } => tools += 1,
            ModelStep::Delegate { .. } => delegations += 1,
            ModelStep::Preamble { .. } => {}
        }
    }
    if answers > 0 {
        if answers != 1
            || tools != 0
            || delegations != 0
            || !matches!(steps.last(), Some(ModelStep::Answer { .. }))
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
    } else if tools > 0 {
        if delegations != 0 || tools > MAX_BATCH_TOOL_CALLS {
            return Err(AgentFailure::InvalidModelOutput);
        }
    } else if delegations > 0 {
        if delegations != 1 {
            return Err(AgentFailure::InvalidModelOutput);
        }
    } else {
        return Err(AgentFailure::InvalidModelOutput);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn preamble() -> ModelStep {
        ModelStep::Preamble {
            text: "Reading".into(),
        }
    }
    fn answer() -> ModelStep {
        ModelStep::Answer {
            text: "Answer".into(),
            artifacts: vec![],
        }
    }
    fn tool() -> ModelStep {
        ModelStep::CallTool {
            tool_id: "test.read".into(),
            definition_revision: 1,
            input: "{}".into(),
        }
    }
    fn delegate() -> ModelStep {
        ModelStep::Delegate {
            agent_id: "test.expert".into(),
            definition_revision: 1,
            message: "Review".into(),
            context_refs: vec![],
        }
    }

    #[test]
    fn canonical_batch_grammar_accepts_all_three_kinds_at_the_existing_bounds() {
        for executable in [answer(), tool(), delegate()] {
            assert_eq!(validate_model_batch_shape(&[executable.clone()]), Ok(()));
            let mut batch = vec![preamble(); 15];
            batch.push(executable);
            assert_eq!(validate_model_batch_shape(&batch), Ok(()));
        }
        let mut batch = vec![preamble(); 8];
        batch.extend(vec![tool(); 8]);
        assert_eq!(validate_model_batch_shape(&batch), Ok(()));
    }

    #[test]
    fn canonical_batch_grammar_rejects_mixed_trailing_and_oversized_batches() {
        for batch in [
            vec![],
            vec![preamble()],
            vec![tool(), preamble()],
            vec![answer(), preamble()],
            vec![delegate(), preamble()],
            vec![answer(), tool()],
            vec![tool(), answer()],
            vec![answer(), delegate()],
            vec![delegate(), answer()],
            vec![tool(), delegate()],
            vec![delegate(), tool()],
            vec![answer(), answer()],
            vec![delegate(), delegate()],
            vec![tool(); 9],
        ] {
            assert_eq!(
                validate_model_batch_shape(&batch),
                Err(AgentFailure::InvalidModelOutput),
                "{batch:?}"
            );
        }
        let mut batch = vec![preamble(); 16];
        batch.push(answer());
        assert_eq!(
            validate_model_batch_shape(&batch),
            Err(AgentFailure::InvalidModelOutput)
        );
    }
}
