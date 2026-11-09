# React + TypeScript + Vite

This template provides a minimal setup to get React working in Vite with HMR and lint rules.

Currently, two official plugins are available:

- [@vitejs/plugin-react](https://github.com/vitejs/vite-plugin-react/blob/main/packages/plugin-react) uses [Oxc](https://oxc.rs)
- [@vitejs/plugin-react-swc](https://github.com/vitejs/vite-plugin-react/blob/main/packages/plugin-react-swc) uses [SWC](https://swc.rs/)

## React Compiler

The React Compiler is not enabled on this template because of its impact on dev & build performances. To add it, see [this documentation](https://react.dev/learn/react-compiler/installation).

## Linting

`bun run lint` runs [oxlint](https://oxc.rs) with type-aware rules via `oxlint-tsgolint`, which embeds the Go-based TypeScript compiler and therefore works with the TypeScript 7 toolchain this project uses (typescript-eslint does not). Rules live in `.oxlintrc.json`; see [docs/agents/frontend-code-quality.md](../docs/agents/frontend-code-quality.md) for the rationale behind each one.
