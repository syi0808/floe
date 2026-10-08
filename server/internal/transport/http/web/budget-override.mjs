function normalizedEndpoint(value) {
  return String(value || '').trim().replace(/\/+$/, '');
}

export function budgetOverrideForTarget(profile, purpose, model, baseURL) {
  const configured = profile?.purposes?.[purpose];
  const budgetOverride = configured?.budget_override;
  if (!budgetOverride) return {budgetOverride: null, cleared: false};
  if (configured.model !== model || normalizedEndpoint(profile.base_url) !== normalizedEndpoint(baseURL)) {
    return {budgetOverride: null, cleared: true};
  }
  return {budgetOverride, cleared: false};
}
