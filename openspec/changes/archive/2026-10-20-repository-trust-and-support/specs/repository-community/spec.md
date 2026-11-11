## ADDED Requirements

### Requirement: Repository licensing matches the declared workspace license

The repository root SHALL include the complete standard GNU General Public
License version 3 text. The Cargo workspace and its member crates SHALL
declare `GPL-3.0-only`. Included first-party components SHALL identify GPL
version 3 only in their metadata and source notices. KDE plugin metadata SHALL
use KDE's recognized `GPLv3` keyword, and SPDX source notices SHALL use
`GPL-3.0-only`.

#### Scenario: A visitor checks the source license

- **WHEN** a visitor opens the repository root on GitHub
- **THEN** the standard GPL version 3 text is available there
- **AND** the root workspace and its member crates identify the license as
  `GPL-3.0-only`

### Requirement: Users can find safe support and contribution instructions

The repository SHALL provide concise support, contribution and security
reporting instructions in English and Spanish. Instructions SHALL avoid
requesting clipboard contents, secrets or private user data.

#### Scenario: A user reports an application problem

- **WHEN** a user follows the repository's support route
- **THEN** it directs them to the relevant setup guide or GitHub issue form
- **AND** the issue form asks for useful version and platform context
- **AND** it warns users to remove personal clipboard data and secrets

#### Scenario: A user reports a security issue

- **WHEN** a user follows the security reporting instructions
- **THEN** the instructions name a private channel only after its availability
  has been confirmed
- **AND** they do not direct sensitive details to a public issue

#### Scenario: No private security channel is configured

- **WHEN** the repository has no enabled private reporting feature or
  maintainer-provided private contact
- **THEN** the security policy states that detailed reports cannot currently
  be accepted through a private route
- **AND** it does not ask the user to disclose vulnerability details publicly

#### Scenario: A contributor prepares a change

- **WHEN** a contributor follows the contribution guide
- **THEN** it links to the current development instructions and repository
  contribution expectations
- **AND** it does not claim that unimplemented features or unsupported
  platforms are accepted project capabilities
