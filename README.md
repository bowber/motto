# Motto: The Minimalist Bit-Level Toolchain

**Motto** turns your Rust structs into high-performance, bit-packed binary protocols with automated multi-platform SDK generation. Optimized for extreme efficiency on low-resource environments (e.g., 2GB RAM VPS).

## Why Motto?

**The Problem**: You're building a real-time multiplayer game or IoT system. You need:
- Consistent data types across server, web, mobile, and game clients
- Binary protocols that don't waste bandwidth
- Infrastructure that doesn't cost $500/month

**The Solution**: Define your types once in plain Rust. Motto generates everything else.

```rust
// src/schema.rs
// No serde, no manual routing, no boilerplate.

struct Player {
    id: u64,
    position: Position,
    health: u8,
}

struct Position {
    x: f32,
    y: f32,
}

struct ChatMessage {
    from: u64,
    content: String,
}

// Motto automatically aggregates these into a bit-optimized message router.
```

## Features

- **Zero Dependencies in Schema**: No `serde` derives required. Just plain Rust structs.
- **Implicit Message Router**: Individual structs are automatically aggregated into a single, bit-optimized router enum.
- **Bit-Level Packing**: Computes minimal bit-width for enum variants, skipping standard byte-alignment where possible.
- **A/B Deployment Ready**: 1-byte version header enables automatic traffic routing between protocol versions on your infrastructure.
- **Infrastructure Agnostic**: Works with WebTransport, WebSocket, NATS Core, or raw TCP. Bring your own transport.
- **Multi-Platform SDKs**: TypeScript/WASM, Swift, Kotlin, Unity/C# — all from one schema.

## Installation

```bash
cargo install motto --bin motto-cli
```

Or build from source:

```bash
git clone https://github.com/bowber/motto
cd motto
cargo build --release
```

## Quick Start

### 1. Initialize a project

```bash
motto-cli init --path my-project
```

This creates:
- `src/schema.rs` - Your schema definitions
- `motto.lock` - Version tracking file
- `generated/` - Output directory

### 2. Define your schema

```rust
// src/schema.rs
// Clean, minimal, no ceremony.

struct Player {
    id: u64,
    position: Position,
    health: u8,
}

struct Position {
    x: f32,
    y: f32,
}

struct PlayerJoined {
    player: Player,
}

struct PlayerMoved {
    player_id: u64,
    position: Position,
}

struct PlayerLeft {
    player_id: u64,
}

// That's it. Motto handles the rest.
```

### 3. Generate SDKs

```bash
# Generate all platforms
motto-cli generate

# Generate specific platforms
motto-cli generate --targets typescript,swift

# With WASM bindings
motto-cli generate --wasm
```

### 4. Lock the schema version

```bash
motto-cli lock --bump minor
```

## Architecture

Motto follows a three-phase compiler architecture:

### 1. Static Analysis Frontend
- Parses plain Rust structs (no macro annotations required)
- Computes schema fingerprint for change detection

### 2. Intermediate Representation (IR)
- **Implicit Routing**: Automatically aggregates individual structs into a single, bit-optimized router enum
- **Bit-Level Packing**: Computes minimal bit-width for enum variants and field offsets, skipping standard byte-alignment where possible
- Generates language-agnostic manifest with field offsets

### 3. Backend Emitters
- **TypeScript/WASM**: ESM-compliant TypeScript with conditional exports for WASM or Native Addon
- **Swift**: Native iOS/macOS SDK with Codable conformance
- **Kotlin**: Android/JVM SDK with kotlinx.serialization support
- **Unity/C#**: C# wrappers with unsafe pointers for memory-efficient DllImport

## Deployment Philosophy: The $5 Stack

Motto isn't just about code generation; it's about making distributed systems affordable. By pairing the generated SDK with the **Mottomesh Template**, you can run a full production stack:

| Component | Implementation | Memory |
|-----------|---------------|--------|
| Gateway | Rust WebTransport/WebSocket bridge | ~30MB |
| Message Bus | NATS Core (stateless) | ~20MB |
| Game Server | Your Rust logic | ~50MB |
| **Total** | | **<100MB** |

This allows you to host a real-time cluster on a **$5/mo VPS** without breaking a sweat.

### A/B Deployment with Version Routing

The 1-byte version header isn't just for "detecting" protocol changes — it enables **automatic traffic routing**:

```
┌─────────────┐     ┌──────────────────┐     ┌─────────────┐
│   Client    │────▶│  Gateway/Sidecar │────▶│  Server v2  │
│ (version 2) │     │   Routes by Ver  │     └─────────────┘
└─────────────┘     │                  │     ┌─────────────┐
                    │                  │────▶│  Server v1  │
┌─────────────┐     │                  │     │  (legacy)   │
│   Client    │────▶│                  │     └─────────────┘
│ (version 1) │     └──────────────────┘
└─────────────┘
```

Deploy new versions alongside old ones. Migrate clients gradually. Zero downtime.

## CLI Commands

| Command | Description |
|---------|-------------|
| `init` | Initialize a new motto project |
| `generate` | Generate SDK code from schema.rs |
| `check` | Check schema for breaking changes |
| `lock` | Update motto.lock with new schema fingerprint |
| `watch` | Watch for schema changes and regenerate |

## Generated Output Structure

```
generated/
├── typescript/
│   ├── package.json
│   └── src/
│       ├── types.ts      # Type definitions
│       ├── codec.ts      # Binary encoding/decoding
│       ├── runtime.ts    # State machine, transport
│       └── index.ts      # Exports
├── swift/
│   ├── Package.swift
│   └── Sources/MottoSDK/
│       ├── Types.swift
│       ├── Codec.swift
│       └── Runtime.swift
├── kotlin/
│   ├── build.gradle.kts
│   └── src/main/kotlin/io/motto/sdk/
│       ├── Types.kt
│       ├── Codec.kt
│       └── Runtime.kt
└── unity/MottoSDK/
    ├── Motto.SDK.asmdef
    └── Runtime/
        ├── Types.cs
        ├── Codec.cs
        ├── Runtime.cs
        └── NativeBridge.cs
```

## Runtime Features

The generated SDKs include:

- **PacketBuilder/PacketView**: Zero-copy packet construction and parsing
- **State Machine**: Connection state management with retry logic
- **Compression**: Optional Zstd compression/decompression
- **Transport Abstraction**: Plug in WebTransport, WebSocket, NATS, or raw TCP

## Supported Types

| Rust Type | TypeScript | Swift | Kotlin | C# |
|-----------|------------|-------|--------|-----|
| `u8`/`i8` | `number` | `UInt8`/`Int8` | `UByte`/`Byte` | `byte`/`sbyte` |
| `u16`/`i16` | `number` | `UInt16`/`Int16` | `UShort`/`Short` | `ushort`/`short` |
| `u32`/`i32` | `number` | `UInt32`/`Int32` | `UInt`/`Int` | `uint`/`int` |
| `u64`/`i64` | `bigint` | `UInt64`/`Int64` | `ULong`/`Long` | `ulong`/`long` |
| `f32`/`f64` | `number` | `Float`/`Double` | `Float`/`Double` | `float`/`double` |
| `bool` | `boolean` | `Bool` | `Boolean` | `bool` |
| `String` | `string` | `String` | `String` | `string` |
| `Vec<T>` | `T[]` | `[T]` | `List<T>` | `T[]` |
| `Option<T>` | `T \| undefined` | `T?` | `T?` | `T?` |
| `HashMap<K,V>` | `Map<K,V>` | `[K: V]` | `Map<K,V>` | `Dictionary<K,V>` |

## License

MIT OR Apache-2.0
