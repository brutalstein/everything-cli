# Step 11 Provider Resilience Notes

Implementation notes for the in-progress Step 11 branch. The normative architecture remains under `docs/` and `STATUS.md` tracks acceptance.

- Hard eligibility constraints are evaluated before utility optimization.
- Capability snapshots are freshness-bound.
- Retry and failover have independent hard bounds.
- Endpoint health and rate-limit state are endpoint-scoped.
- Cost accounting uses integer micro-USD arithmetic and rounds up fractional charges.
- ProviderBench/RouterBench use deterministic scripted fixtures only.
