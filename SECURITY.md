# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in OpenHouse (smart contract, settler service, core library, or frontend), please report it responsibly by opening a private security advisory on GitHub or contacting the maintainer directly.

Please include:
- A clear description of the vulnerability and its potential impact.
- Steps to reproduce or a proof-of-concept.
- Affected files, instructions, or endpoints.

## Scope

In scope:
- Solana program (`programs/openhouse`)
- Settler service (`services/settler` / `api/index.js`)
- Core fairness & serialization library (`packages/core`)
- Frontend client (`index.html`, `vendor/`)

Out of scope:
- Third-party RPC providers, wallet extensions, or hosting infrastructure
- Devnet test funds or practice mode

## Bounty Policy

OpenHouse is an open-source project currently running on Solana devnet. There is no bug bounty program or monetary reward promised for vulnerability reports.
