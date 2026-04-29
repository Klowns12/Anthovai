export const klangDocs = [
  {
    id: 'introduction',
    title: '1. Introduction',
    content: `
Klang (Klowns Language) is a modern, statically-typed, compiled programming language designed for the Kaiser Klowns ecosystem. It covers every domain: Web, AI, Backend, Mobile, Desktop, Cloud, and OS development.

### Design Goals
* **Speed of C / Rust** — compiles to native LLVM IR, C, or WebAssembly
* **Readability of Python** — clean, expressive syntax with minimal boilerplate
* **Structure of TypeScript** — strong static typing with full type inference
* **Scalability of Go** — first-class concurrency, channels, and module system

### File Extension
All Klang source files use the \`.kkg\` extension — e.g. \`main.kkg\`

### Kaiser Klowns Ecosystem
| Product | Role | Status |
|---|---|---|
| **Klang** | Core programming language (\`.kkg\`) | ✅ Complete |
| **Aello** | Integrated Development Environment | → Next |
| **Arkai AI** | AI assistant built with Klang AI primitives | Planned |
| **Alfa Git** | Version control platform | Planned |
| **Marisea Cloud** | Cloud deployment & serverless runtime | Planned |
| **Ocean OS** | Operating system kernel written in Klang | Planned |
    `
  },
  {
    id: 'installation',
    title: '2. Installation',
    content: `
### 2.1 Requirements
* \`gcc 10+\` — the only dependency (available on every OS)
* No Rust, no LLVM, no virtual machine required
* Supported: Windows, macOS, Linux, ARM (Apple Silicon)

### 2.2 Install

**macOS / Linux**
\`\`\`bash
curl -sSf https://klang.kkg/install.sh | sh
\`\`\`

**Windows**
\`\`\`bash
winget install KaiserKlowns.Klang
\`\`\`

**Build from Source (gcc only, no Rust needed)**
\`\`\`bash
git clone https://github.com/kaiser-klowns/klang
cd klang
make bootstrap    # gcc only — Rust not required
./klangc --version
# klangc v1.0.0 — Kaiser Klowns Dragon
\`\`\`

> **Tip:** After Phase 2, Klang only needs \`gcc\` — which ships with every major OS. No rustup, no cargo, no LLVM required.
    `
  },
  {
    id: 'quick-start',
    title: '3. Quick Start',
    content: `
### 3.1 Hello World
\`\`\`kkg
// hello.kkg
fn main() {
  println("Hello Kaiser Klowns!")
}
\`\`\`

\`\`\`bash
klang run hello.kkg
# Hello Kaiser Klowns!
\`\`\`

### 3.2 New Project
\`\`\`bash
kpkg init my-app
cd my-app
klang run main.kkg
\`\`\`

### 3.3 Project Structure
\`\`\`bash
my-app/
├── main.kkg          # entry point
├── klang.toml        # project manifest
├── klang.lock        # dependency lock file
└── src/              # additional source files
\`\`\`
    `
  },
  {
    id: 'language-tour',
    title: '4. Language Tour',
    content: `
### 4.1 Variables & Types
Variables are immutable by default. Use \`mut\` for mutable bindings, \`const\` for compile-time constants.

\`\`\`kkg
let name = "Klang"         // immutable, type inferred
let mut count = 0            // mutable
const MAX: int = 1024        // compile-time constant
let age: int = 25            // explicit type
let ratio: float = 3.14
let active: bool = true
let opt: ?int = nil          // optional (nullable)
\`\`\`

**Built-in Types**
| Type | Description | Example |
|---|---|---|
| \`int\` | 64-bit signed integer | \`42\`, \`-7\`, \`0\` |
| \`float\` | 64-bit floating point | \`3.14\`, \`-0.5\` |
| \`bool\` | Boolean value | \`true\`, \`false\` |
| \`string\` | UTF-8 string | \`"hello"\`, \`"🐉"\` |
| \`char\` | Single UTF-8 character | \`'a'\`, \`'🐉'\` |
| \`[T]\` | Dynamic array of type T | \`[1, 2, 3]\` |
| \`map[K]V\` | HashMap from K to V | \`map["a": 1]\` |
| \`?T\` | Optional / Nullable T | \`nil\` or value |
| \`(A, B)\` | Tuple | \`(1, "ok")\` |

### 4.2 Functions
\`\`\`kkg
fn add(a: int, b: int) -> int { return a + b }

// Arrow shorthand
fn square(x: int) -> int => x * x

// Async function
async fn fetchUser(id: int) -> Result<User> {
  let res = await http.get("/users/" + id)
  return Ok(res.json())
}

// Lambda / higher-order
let doubled = [1,2,3,4,5].map(x => x * 2)  // [2,4,6,8,10]
\`\`\`

### 4.3 Control Flow
\`\`\`kkg
if score >= 90 { println("A") }
else if score >= 80 { println("B") }
else { println("C") }

for i in 0..10 { println(i) }
for item in ["a","b","c"] { println(item) }
while count > 0 { count -= 1 }

match status {
  200 => println("OK")
  404 => println("Not Found")
  _   => println("Other: " + status)
}
\`\`\`

### 4.4 Structs & Enums
\`\`\`kkg
struct User { name: string, age: int, email: string }
let user = User("Alice", 25, "alice@klowns.dev")

impl User {
  fn greet(self) -> string => "Hi, I am " + self.name
}

enum Status { Active, Inactive, Pending(string) }
let s = Status.Pending("review")

match s {
  Status.Active     => println("live")
  Status.Pending(m) => println("Pending: " + m)
  _                 => println("off")
}
\`\`\`

### 4.5 Traits & Generics
\`\`\`kkg
trait Drawable {
  fn draw(self)
  fn area(self) -> float
}

impl Drawable for Circle {
  fn draw(self) { println("Circle r=" + self.radius) }
  fn area(self) -> float => 3.14159 * self.radius * self.radius
}

fn first<T>(list: [T]) -> ?T {
  if list.len() == 0 { return nil }
  return list[0]
}
\`\`\`

### 4.6 Error Handling
\`\`\`kkg
fn divide(a: float, b: float) -> Result<float> {
  if b == 0.0 { return Err("division by zero") }
  return Ok(a / b)
}

match divide(10.0, 2.0) {
  Ok(v)    => println(v)
  Err(msg) => println("Error: " + msg)
}

// ? operator — propagate error early
fn calc(x: float) -> Result<float> {
  let a = divide(x, 2.0)?
  return Ok(a * 3.0)
}
\`\`\`

### 4.7 Async & Concurrency
\`\`\`kkg
async fn fetchData(url: string) -> string {
  let res = await http.get(url)
  return res.body
}

// Parallel tasks
let task1 = spawn fetchUsers()
let task2 = spawn fetchProducts()
let (users, products) = await join(task1, task2)

// Channels
let ch = channel<string>()
spawn { ch.send("hello") }
let msg = ch.receive()
\`\`\`

### 4.8 Memory & Ownership
Each value has exactly one owner. The borrow checker enforces safety at compile time — no garbage collector.
\`\`\`kkg
let s1 = "hello"
let s2 = s1          // s1 MOVED to s2
// println(s1)       // ERROR: use after move

let s2 = s1.clone()  // explicit copy — both valid

fn print_len(s: &string) { println(s.len()) }
let name = "Klang"
print_len(&name)     // borrow — name still valid
\`\`\`

### 4.9 Modules
\`\`\`kkg
use std::io
use std::math::{ sqrt, PI }
use ./services::user::{ UserService }
use @klowns/auth::{ jwt }

pub fn createUser(name: string) -> User { ... }
pub const VERSION: string = "1.0.0"
\`\`\`
    `
  },
  {
    id: 'web-server',
    title: '5. Web Server',
    content: `
Klang has first-class web server syntax via the \`app\` block. No external framework is needed.

\`\`\`kkg
app server {
  port: 8080

  route "/" {
    return json({ message: "Klang is alive!", version: "1.0.0" })
  }

  route "/health" {
    return json({ status: "ok", uptime: sys.uptime() })
  }

  route POST "/users" {
    let body = request.json()
    let user = db.create(body)
    return json({ created: true, id: user.id })
  }
}
\`\`\`
    `
  },
  {
    id: 'ai-machine-learning',
    title: '6. AI & Machine Learning',
    content: `
Klang is the first general-purpose language with AI primitives as core syntax — not a library.

\`\`\`kkg
ai model VisionAI {
  dataset: load("./data/images")
  architecture: CNN
  epochs: 50
  learning_rate: 0.001
  train: auto
}

fn main() {
  let model  = ai.load(VisionAI)
  let result = model.predict("cat.jpg")
  println("Label: " + result.label)
  println("Confidence: " + result.confidence)
}
\`\`\`
    `
  },
  {
    id: 'standard-library',
    title: '7. Standard Library',
    content: `
### 7.1 std::io
| Function | Description |
|---|---|
| \`io.read(path)\` | Read file as string |
| \`io.write(path, content)\` | Write string to file |
| \`io.append(path, content)\` | Append to file |
| \`io.exists(path)\` | Check if path exists → bool |
| \`io.delete(path)\` | Delete file or empty dir |
| \`io.list_dir(path)\` | List directory → [string] |
| \`io.make_dir(path)\` | Create directory (recursive) |
| \`io.read_lines(path)\` | Read as array of lines |

### 7.2 std::math
| Function / Constant | Description |
|---|---|
| \`math.sqrt(x)\` | Square root |
| \`math.pow(x, n)\` | Power: x^n |
| \`math.abs(x)\` | Absolute value |
| \`math.min/max(a, b)\` | Minimum / Maximum |
| \`math.floor/ceil(x)\` | Round down / up |
| \`math.round(x)\` | Round to nearest integer |
| \`math.sin/cos(x)\` | Trigonometric functions |
| \`math.log/log2(x)\` | Natural / base-2 logarithm |
| \`math.PI\` | π = 3.14159265358979... |
| \`math.E\` | e = 2.71828182845904... |

### 7.3 std::http
| Function | Description |
|---|---|
| \`http.get(url)\` | HTTP GET → Response |
| \`http.post(url, body)\` | HTTP POST with JSON body |
| \`http.put(url, body)\` | HTTP PUT |
| \`http.delete(url)\` | HTTP DELETE |
| \`response.json()\` | Parse body as JSON |
| \`response.text()\` | Get body as string |
| \`response.status\` | HTTP status code (int) |

### 7.4 std::json
\`\`\`kkg
let obj    = json.parse('{"name":"Klang"}')
let name   = obj["name"]
let text   = json.stringify(obj)
let pretty = json.pretty(obj)
\`\`\`
    `
  },
  {
    id: 'cli-reference',
    title: '8. CLI Reference',
    content: `
| Command | Description |
|---|---|
| \`klang run file.kkg\` | Run a .kkg file directly |
| \`klang --compile file.kkg\` | Compile to C source |
| \`klang --llvm file.kkg\` | Compile to LLVM IR |
| \`klang --wasm file.kkg\` | Compile to WebAssembly (.wat) |
| \`klang --check file.kkg\` | Type-check only |
| \`klang fmt\` | Format all .kkg files |
| \`klang lint\` | Lint and report warnings |
| \`klang test\` | Run test suite |
| \`klang repl\` | Start interactive REPL |
| \`klang --version\` | Show version information |

### 8.1 REPL Commands
| Command | Description |
|---|---|
| \`:help\` | Show all REPL commands |
| \`:quit / :exit\` | Exit the REPL |
| \`:type <expr>\` | Show inferred type of expression |
| \`:load file.kkg\` | Load and evaluate a file |
| \`:reset\` | Reset all bindings |
| \`:multiline\` | Toggle multi-line input mode |
    `
  },
  {
    id: 'kpkg',
    title: '9. kpkg — Package Manager',
    content: `
### 9.1 Commands
| Command | Description |
|---|---|
| \`kpkg init <name>\` | Create new Klang project |
| \`kpkg add <pkg>\` | Add a dependency |
| \`kpkg remove <pkg>\` | Remove a dependency |
| \`kpkg install\` | Install all from klang.toml |
| \`kpkg build\` | Build the project |
| \`kpkg run\` | Build and run |
| \`kpkg test\` | Run tests |
| \`kpkg publish\` | Publish to registry |
| \`kpkg search <query>\` | Search the package registry |

### 9.2 klang.toml
\`\`\`toml
[project]
name    = "my-app"
version = "0.1.0"

[dependencies]
klang-web       = "^1.0.0"
"@klowns/auth" = "^2.1.0"

[build]
target   = "native"    # native | wasm | c
optimize = "release"   # debug | release
\`\`\`

### 9.3 Built-in Packages
| Package | Description |
|---|---|
| \`std::http\` | HTTP client & server |
| \`std::json\` | JSON parsing & generation |
| \`std::io\` | File & stream I/O |
| \`std::math\` | Math functions & constants |
| \`std::crypto\` | Hashing, encryption, JWT |
| \`std::db\` | Database abstraction layer |
| \`std::log\` | Structured logging |
| \`std::cli\` | CLI argument parsing |
| \`std::async\` | Async runtime utilities |
| \`std::web\` | Full-stack web framework |
| \`std::ai\` | AI & ML primitives |
| \`std::test\` | Testing framework |
    `
  },
  {
    id: 'architecture',
    title: '10. Compiler Architecture',
    content: `
### 10.1 Compilation Pipeline
\`\`\`
Source (.kkg)
    |
    V  Lexer        -- tokenize source into tokens
    |
    V  Parser       -- Pratt parser, 10 precedence levels -> AST
    |
    V  Type Checker -- ownership, borrow checker, generics
    |
    +---> C Codegen    -> .c  -> gcc  -> native binary
    |
    +---> LLVM Codegen -> .ll -> llc  -> native binary
    |
    +---> WASM Codegen -> .wat -> browser / edge compute
\`\`\`

### 10.2 Self-Hosting Bootstrap
| Stage | Compiler | Input | Output | Status |
|---|---|---|---|---|
| **Stage 0** | Rust (last time) | bootstrap/*.kkg | klangc.c | Done |
| **Stage 1** | gcc | klangc.c + cruntime/ | klangc binary | Done |
| **Stage 2** | klangc (Klang!) | bootstrap/*.kkg | klangc_v2.c | Done |
| **Verify** | diff | v1.c vs v2.c | IDENTICAL | Done 🐉 |

### 10.3 C Runtime (cruntime/)
| File | Provides |
|---|---|
| \`runtime.h / .c\` | Arena allocator, KlangArray, 20+ string ops, I/O |
| \`klang_stdlib.h / .c\` | Math, file I/O, path, JSON, base64, SHA... |
| \`klang_async.h / .c\` | Cross-platform threads, channels, mutexes |
| \`test_runtime.c\` | 93-assertion test suite for the entire C runtime |
| \`Makefile\` | Cross-platform build — gcc only, no Rust required |

> **Phase 1 Complete:** 93/93 tests passed with gcc 15.2.0 on Windows. C Runtime is fully operational. Rust is no longer needed at runtime.
    `
  },
  {
    id: 'error-reference',
    title: '11. Error Reference',
    content: `
Klang uses Rust-style error messages with ANSI colors, source context, column underlines, and helpful notes.

### 11.1 Error Format
\`\`\`
error[E0001]: use of moved value: \`name\`
  --> src/main.kkg:12:10
   |
11 |   let s2 = name
   |            ---- value moved here
12 |   println(name)
   |           ^^^^ value used after move
   |
   = note: move occurs because \`name\` has type \`string\`
   = help: use \`name.clone()\` to make an explicit copy
\`\`\`

### 11.2 Error Code Categories
| Range | Category |
|---|---|
| \`E0001 - E0099\` | Syntax errors |
| \`E0100 - E0199\` | Type errors |
| \`E0200 - E0299\` | Ownership & borrow checker |
| \`E0300 - E0399\` | Module & import errors |
| \`E0400 - E0499\` | Trait & impl errors |
| \`E0500 - E0599\` | Generic type errors |
| \`E0600 - E0699\` | Async / concurrency errors |
| \`E0700 - E0799\` | Pattern match errors |
| \`E9000 - E9999\` | Internal compiler errors (ICE) |
    `
  },
  {
    id: 'roadmap',
    title: '12. Development Roadmap',
    content: `
| Phase | Name | Status |
|---|---|---|
| **Phase 0** | Bootstrap Compiler (Rust) | Done |
| **Phase 1** | Core Language + Stdlib | Done |
| **Phase 2** | Compiler Backends (LLVM, C, WASM) | Done |
| **Phase 3** | Self-Hosting Bootstrap | Done |
| **Phase 4** | Enterprise Features | Done |
| **Phase 5** | Production Completion | Done |
| **Phase 6** | Completion (All Levels) | Done |
| **Phase 7** | Platform & Ecosystem | Done |
| **Phase 2B** | Remove Rust — gcc-only install | In Progress |
| **Next** | Aello (IDE) | Planned |
| **Future** | Arkai AI, Alfa Git, Marisea Cloud, Ocean OS | Planned |
    `
  }
]
