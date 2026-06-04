# Morphis 🪐

[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**Morphis** is a high-performance, dynamic GraphQL engine written in Rust. It eliminates the need to write boilerplate CRUD code by automatically morphing your database tables into a fully functional GraphQL API driven entirely by a declarative configuration file (`config.yaml`).

Equipped with a powerful dynamic query builder, Morphis supports standard automated CRUD operations while retaining the flexibility to execute highly customized, ad-hoc raw SQL queries safely.

---

## ✨ Features

*   ⚙️ **Configuration-Driven:** Define your tables, columns, and data types in a simple YAML config. No Rust recompilation required to update your schema.
*   🚀 **Blazing Fast:** Built on top of the robust asynchronous foundations of **Axum**, **Async-graphql**, and **SQLx**.
*   🔄 **Automated CRUD:** Instantly exposes queries for reading (single/list) and mutations for creating, updating, and deleting records.
*   🔮 **Custom SQL Execution:** Run customized, ad-hoc raw SQL queries directly through GraphQL when standard CRUD isn't enough.
*   🧩 **Type Mapping:** Dynamically maps relational database types to GraphQL scalars seamlessly at runtime.

---

## 🛠️ System Architecture

```text
  ┌────────────────┐
  │  config.yaml   │ ─── (Defines Tables, Columns & Permissions)
  └───────┬────────┘
          │
          ▼
┌──────────────────┐      ┌────────────────────────┐      ┌──────────────┐
│  GraphQL Client  │ ───> │  Morphis Dynamic Engine│ ───> │ Relational DB│
│  (Queries/Muts)  │ <─── │  (Schema & SQL Builder)│ <─── │ (Postgres/..)│
└──────────────────┘      └────────────────────────┘      └──────────────┘
