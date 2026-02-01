# Motto

**Compiler-as-a-Service: Turn Rust `schema.rs` into multi-platform SDK toolkits**

Motto is a code generation tool that transforms Rust struct and enum definitions into platform-specific SDK code for multiple targets: TypeScript/WASM, Swift, Kotlin, and Unity/C#.

## Architecture

Motto follows a three-phase compiler architecture:

### 1. Static Analysis Frontend
- Uses the `syn` crate to parse Rust AST
- Extracts structs and enums annotated with `#[derive(Serialize, Deserialize)]`
- Computes SHA-256 fingerprint for detecting schema changes

### 2. Intermediate Representation (IR)
- Generates language-agnostic JSON/BSON manifest
- Includes field offsets and bit-alignment requirements
- Supports packed and aligned layouts for Bitcode backend

### 3. Backend Emitters
- **TypeScript/WASM**: ESM-compliant TypeScript with conditional exports for WASM or Native Addon (napi-rs)
- **Swift**: Native iOS/macOS SDK with Codable conformance
- **Kotlin**: Android/JVM SDK with kotlinx.serialization support
- **Unity/C#**: C# wrappers with unsafe pointers for memory-efficient DllImport

## Features

- 🔒 **Single-Version Policy**: 1-byte version header in all packets for sidecar routing
- 📦 **Zero-Copy Interfaces**: Efficient packet framing across all platforms
- 🔄 **Schema Fingerprinting**: SHA-256 hashes detect breaking changes
- 📋 **motto.lock**: Immutable versioning authority for schema evolution
- 🗜️ **Zstd Compression**: Built-in compression support in runtime
- 📡 **WebTransport Ready**: Transport layer abstraction included

## Installation

```bash
cargo install motto --bin motto-cli
```

Or build from source:

```bash
git clone https://github.com/mottomesh/motto
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
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: u64,
    pub name: String,
    pub position: Position,
    pub health: u8,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameEvent {
    PlayerJoined { player: Player },
    PlayerMoved(u64, Position),
    PlayerLeft { player_id: u64 },
}
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

## Protocol Version Byte

All generated packets include a 1-byte version header derived from `motto.lock`:

```
+--------+--------------------+
| Ver(1) |     Payload        |
+--------+--------------------+
```

This enables:
- Sidecar routing based on protocol version
- Backward compatibility detection
- Traffic multiplexing by version

## Runtime Features

The generated SDKs include:

- **PacketBuilder/PacketView**: Zero-copy packet construction and parsing
- **State Machine**: Connection state management with retry logic
- **Compression**: Zstd compression/decompression (where supported)
- **Transport Abstraction**: WebTransport-ready interfaces

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
