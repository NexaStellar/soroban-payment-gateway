# Candidate backlog

These are draft engineering tasks, not active Drips Wave issues. Confirm the
repository's Wave status and add issues through the Drips maintainer dashboard
only after approval. Assign complexity there; do not treat labels or point
estimates in this file as Wave configuration.

## Support merchant profile updates

Allow the administrator to update a registered merchant's display name and
receiving address.

Acceptance criteria:

- Only the configured administrator can update a merchant.
- Updating a profile preserves its active status, volume, and transaction count.
- A missing merchant returns a clear contract error.
- Emit an event containing the merchant ID and changed profile values.
- Add authorization, state-preservation, and missing-merchant tests.
- Document the new entry point and event.

## Define and enforce input size limits

Specify practical maximum lengths for merchant IDs, merchant names, and order
IDs, then enforce them consistently at contract entry points.

Acceptance criteria:

- Document the chosen limits and rationale.
- Reject empty or over-limit values before storage writes or token transfers.
- Apply the same merchant/order ID rules to registration, payment, and escrow
  flows where applicable.
- Add boundary tests for empty, maximum-length, and over-limit values.

