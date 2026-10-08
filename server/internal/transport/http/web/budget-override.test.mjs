import test from 'node:test';
import assert from 'node:assert/strict';
import {budgetOverrideForTarget} from './budget-override.mjs';

const override = {schema_version: 1, context_window_tokens: 8192};
const profile = {
  base_url: 'https://provider.example/v1',
  purposes: {quick_response: {model: 'model-a', budget_override: override}},
};

test('preserves a measured override when provider target identity is unchanged', () => {
  const result = budgetOverrideForTarget(profile, 'quick_response', 'model-a', 'https://provider.example/v1/');
  assert.equal(result.budgetOverride, override);
  assert.equal(result.cleared, false);
});

test('clears a measured override when the model changes', () => {
  assert.deepEqual(budgetOverrideForTarget(profile, 'quick_response', 'model-b', profile.base_url), {
    budgetOverride: null,
    cleared: true,
  });
});

test('clears a measured override when the endpoint changes', () => {
  assert.deepEqual(budgetOverrideForTarget(profile, 'quick_response', 'model-a', 'https://other.example/v1'), {
    budgetOverride: null,
    cleared: true,
  });
});
