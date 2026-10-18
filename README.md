# autocruise

**Put everyday life on cruise control.**

autocruise lets your AI assistant handle everyday errands for you: compare your grocery basket across supermarkets, find a doctor's appointment, check your train. It runs **locally, on your machine**, and acts with **your own accounts**.

> 🚧 **Early stage.** Design in progress — nothing to install yet.

## What is MCP?

The [Model Context Protocol](https://modelcontextprotocol.io) (MCP) is an open standard that lets AI assistants (Claude, ChatGPT, Cursor…) use external **tools**. An MCP server exposes tools such as `search_products` or `find_appointment`; the assistant decides when to call them.

autocruise is an MCP server that bundles many everyday-life tools in a single install.

## Example

> **You:** Compare this basket at Carrefour, Leclerc and Intermarché: milk, Nutella 750g, pasta 500g.
>
> **Assistant** *(using autocruise)*: Leclerc is cheapest at €8.42, Intermarché €8.71, Carrefour €9.05. Want me to fill the Leclerc cart?

## How it works

```
AI assistant ──MCP──▶ autocruise (your machine) ──▶ official APIs (SNCF, …)
                                                └─▶ your own browser session (Doctolib, supermarkets)
```

- **Official APIs** are used whenever a service provides one.
- Otherwise, autocruise works through **your already-open browser**, as you would by hand: you log in yourself, nothing is stored on a server.
- Anything that commits you (booking, adding to a cart) **asks for confirmation first**. Payment is never automated.

## Planned connectors

| Area | Services |
|---|---|
| Groceries | Carrefour, E.Leclerc, Intermarché — and cross-store basket comparison |
| Health | Doctolib |
| Transport | SNCF, Île-de-France Mobilités |

## Principles

Local only · your session, your browser · no anti-bot circumvention · human-paced requests · confirmation before side effects · no telemetry. Details in [docs/PRINCIPLES.md](docs/PRINCIPLES.md).

## Disclaimer

Independent open-source project, **not affiliated with** any of the services above. Unofficial connectors may break at any time and may conflict with a service's terms of use. Use them for personal purposes, at your own risk.

## License

[MIT](LICENSE)
