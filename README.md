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

## Deployment Philosophy: Scale From Small to Large

Motto is designed to grow with you. The same protocol works whether you're:

- **Prototyping** on a single $5 VPS
- **Launching** with a small cluster
- **Scaling** to millions of concurrent users

The generated SDKs are infrastructure-agnostic — plug them into WebTransport, WebSocket, NATS, Kafka, or raw TCP. No code changes required as you scale.

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

## Using the Generated SDKs

The best practice is to **publish your generated SDK to a package registry** and import it like any other dependency. This keeps your application code clean and your protocol versioned.

### TypeScript / Node.js

**1. Publish to npm (or use a private registry):**

```bash
cd generated/typescript
npm run build
npm publish --access public
# or for private: npm publish --registry https://your-registry.com
```

**2. Install in your application:**

```bash
npm install @motto/schema
# or with your custom package name
```

**3. Use in your code:**

```typescript
import {
  // Types
  Player,
  Position,
  ClientMessage,
  ServerMessage,
  
  // Codec
  encodePosition,
  decodePosition,
  PacketBuilder,
  PacketView,
  PROTOCOL_VERSION_BYTE,
  
  // Runtime
  MottoTransport,
  ConnectionState,
} from '@motto/schema';

// Create a player position
const pos: Position = { x: 100.5, y: 200.3 };

// Encode to binary (includes version byte header)
const encoded = encodePosition(pos);
console.log(`Encoded ${encoded.byteLength} bytes, version: 0x${encoded[0].toString(16)}`);

// Decode back
const decoded = decodePosition(encoded);
console.log(`Position: (${decoded.x}, ${decoded.y})`);

// Build custom packets with PacketBuilder
const builder = new PacketBuilder();
builder.writeU64(BigInt(12345));  // player_id
builder.writeF32(pos.x);
builder.writeF32(pos.y);
const packet = builder.build();

// Connect via WebTransport
const transport = new MottoTransport('https://your-server.com/game');
await transport.connect();
await transport.sendDatagram(packet);
```

### Swift / iOS / macOS

**1. Add as a Swift Package dependency:**

```swift
// In your Package.swift or Xcode project
dependencies: [
    .package(url: "https://github.com/your-org/motto-schema-swift", from: "0.1.0"),
    // Or use a local path during development:
    // .package(path: "../generated/swift")
]
```

**2. Use in your code:**

```swift
import MottoSDK

// Types are ready to use
let position = Position(x: 100.5, y: 200.3)
let player = Player(
    id: 12345,
    name: "Alice",
    position: position,
    velocity: Velocity(dx: 0, dy: 0),
    health: 100,
    score: 0,
    status: .online,
    avatarUrl: nil
)

// Encode to binary
let encoded = Codec.encodePosition(position)
print("Encoded \(encoded.count) bytes")

// Decode back  
let decoded = Codec.decodePosition(encoded)
print("Position: (\(decoded.x), \(decoded.y))")

// Use the transport layer
let transport = MottoTransport(url: "https://your-server.com/game")
try await transport.connect()
try await transport.send(encoded)
```

### Kotlin / Android / JVM

**1. Publish to Maven (or use a local dependency):**

```bash
cd generated/kotlin
./gradlew publishToMavenLocal
# or publish to your Maven repository
```

**2. Add dependency in your app's `build.gradle.kts`:**

```kotlin
dependencies {
    implementation("io.motto:schema:0.1.0")
}
```

**3. Use in your code:**

```kotlin
import io.motto.sdk.*

// Create types
val position = Position(x = 100.5f, y = 200.3f)
val player = Player(
    id = 12345u,
    name = "Alice",
    position = position,
    velocity = Velocity(dx = 0f, dy = 0f),
    health = 100u,
    score = 0u,
    status = PlayerStatus.Online,
    avatarUrl = null
)

// Encode/decode
val encoded = Codec.encodePosition(position)
println("Encoded ${encoded.size} bytes")

val decoded = Codec.decodePosition(encoded)
println("Position: (${decoded.x}, ${decoded.y})")

// Use with coroutines
val transport = MottoTransport("https://your-server.com/game")
transport.connect()
transport.send(encoded)
```

### Unity / C#

**1. Copy the generated SDK to your Unity project:**

```bash
cp -r generated/unity/MottoSDK Assets/Plugins/
```

Or add as a Unity Package (add to `Packages/manifest.json`):

```json
{
  "dependencies": {
    "com.motto.sdk": "file:../../generated/unity/MottoSDK"
  }
}
```

**2. Use in your C# scripts:**

```csharp
using Motto.SDK;
using UnityEngine;

public class GameClient : MonoBehaviour
{
    private MottoTransport transport;

    async void Start()
    {
        // Create types
        var position = new Position { X = 100.5f, Y = 200.3f };
        var player = new Player
        {
            Id = 12345,
            Name = "Alice",
            Position = position,
            Health = 100,
            Score = 0,
            Status = PlayerStatus.Online
        };

        // Encode to binary
        byte[] encoded = Codec.EncodePosition(position);
        Debug.Log($"Encoded {encoded.Length} bytes, version: 0x{encoded[0]:X2}");

        // Decode back
        Position decoded = Codec.DecodePosition(encoded);
        Debug.Log($"Position: ({decoded.X}, {decoded.Y})");

        // Connect and send
        transport = new MottoTransport("wss://your-server.com/game");
        await transport.ConnectAsync();
        await transport.SendAsync(encoded);
    }

    void OnDestroy()
    {
        transport?.Dispose();
    }
}
```

## CI/CD Integration

Automate SDK generation and publishing in your pipeline:

```yaml
# .github/workflows/sdk.yml
name: Generate & Publish SDKs

on:
  push:
    paths: ['src/schema.rs']
    branches: [main]

jobs:
  generate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Install Motto CLI
        run: cargo install motto --bin motto-cli
      
      - name: Check for breaking changes
        run: motto-cli check
      
      - name: Generate SDKs
        run: motto-cli generate
      
      - name: Publish TypeScript SDK
        run: |
          cd generated/typescript
          npm run build
          npm publish
        env:
          NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
      
      - name: Publish Swift SDK
        run: |
          cd generated/swift
          git init && git add .
          git commit -m "SDK v$(cat ../../motto.lock | jq -r .version)"
          git push --force https://x:${{ secrets.GH_TOKEN }}@github.com/your-org/motto-schema-swift main
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
