## ADDED Requirements

### Requirement: Reject drops on peer-bound imported collections

ClipVault SHALL treat a user collection bound to a peer as an ineligible
drag-and-drop target. Releasing a card over such a collection SHALL leave all
entry memberships unchanged and SHALL show a localized explanation. Local
user collections that are not peer-bound SHALL retain the existing drop
behavior. This restriction SHALL NOT change the existing pointer controller,
mouse fallback, cancellation paths, pointer capture, accessible assignment
alternative or opaque entry-id-only drag payload.

#### Scenario: Drop on a peer-bound collection

- **WHEN** the user releases a local history card over a collection bound to
  another peer
- **THEN** no association command is sent and no entry membership changes
- **AND** ClipVault shows a localized message that captures cannot be dropped
  into a collection imported from another team

#### Scenario: Peer-bound collection is not presented as an accepting target

- **WHEN** a card is dragged over a peer-bound collection
- **THEN** the collection does not show valid-target feedback or a copy drop
  effect
- **AND** releasing over it still provides the localized rejection message

#### Scenario: Drop on a local collection

- **WHEN** the user releases a card over a user collection that is not
  peer-bound
- **THEN** the existing additive membership behavior remains unchanged

#### Scenario: Rejected drop preserves the drag contract

- **WHEN** a card is dragged over or released on a peer-bound collection
- **THEN** the drag payload still contains only the opaque entry identifier
- **AND** pointer capture, mouse fallback, text-selection prevention,
  cancellation, ghost cleanup and interactive-control exclusions retain their
  existing behavior
