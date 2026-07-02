# Synapse

Synapse is an MCAT study app built on top of the [Anki](https://apps.ankiweb.net)
core. It keeps Anki's battle-tested spaced-repetition engine (FSRS on the V3
scheduler), sync, and data model, and builds an MCAT-focused studying experience
around it — error-driven card creation, application-level practice, a
prerequisite knowledge graph, grounded AI, and honest readiness metrics.

> The name Synapse reflects what the product is built to do: strengthen the
> connections between concepts the way a synapse strengthens with repeated use.

## About

Synapse is a friendly fork of Anki. The underlying engine, file formats
(`.anki2`, `.apkg`, `.colpkg`), and add-on API remain Anki-compatible, so
existing Anki decks (AnKing, etc.) can be imported directly and collections can
still be exported back to Anki.

## Built on Anki

Synapse is derived from [Anki](https://github.com/ankitects/anki), which is
licensed under the AGPL3. We are grateful to the Anki project and its many
[contributors](./CONTRIBUTORS). Background on the architecture lives in the
upstream [Anki dev docs](https://dev-docs.ankiweb.net) and in
[`notes/ARCHITECTURE.md`](./notes/ARCHITECTURE.md).

## Getting Started

### Development

Every command you need — building, running, testing, linting, formatting — is
defined as a recipe in the project `justfile`. Run `just --list` to see them.

To build and run in development mode:

```
just run
```

For more information on building and developing, please see
[Development](./docs/development.md).

### Contributing

Check out the [Contribution Guidelines](./docs/contributing.md).

## License

Synapse, like Anki, is licensed under the AGPL3: [LICENSE](./LICENSE)
