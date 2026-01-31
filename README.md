# Aether-Store: The Autonomous Software Factory

**Aether-Store** is a next-generation "Self-Building" infrastructure designed for the AI-native era. Unlike traditional software which is static, Aether-Store treats logic as immutable, content-addressed "Atoms" that can be verified, visualized, and optimized autonomously.

> **"In the Electric Era, we don't write code; we orchestrate logic."**

## 🏗️ Architecture: The 8 Pillars

This project was built in 8 distinct phases, each adding a critical capability to the organism:

1.  **The Warehouse (Aether-Store)**: A Content-Addressed Storage (CAS) system using `sled` and `blake3`. Logic is deduplicated and immutable.
2.  **The Motor (Aether-Kernel)**: An execution engine that runs `LogicAtoms`. It supports basic arithmetic and has hooks for financial operations.
3.  **The Truth (Aether-Guard)**: A mathematical verification layer using the `Z3` theorem prover. It enforces "Genesis Laws" (e.g., 0% Riba, Data Sovereignty) at the atomic level.
4.  **The Brain (Aether-Loom)**: A semantic parser that translates human intent (Natural Language) into executable `LogicAtoms`.
5.  **The Orchestrator (Aether-Manifest)**: A build system that reads high-level `manifest.yaml` files and constructs the entire dependency graph.
6.  **The Grid (Visualization)**: A real-time 3D dashboard (Axum + Cytoscape.js) to visualize the software architecture.
7.  **The Self-Healer (Optimizer)**: A feedback loop where the Kernel monitors execution metrics (ns) and triggers the Loom to "evolve" slow nodes.
8.  **The Resonator (Sensory I/O)**: A secure I/O layer that allows fetching external data while enforcing strict Data Sovereignty rules (e.g., "Sovereign data must stay local").

## 🚀 Getting Started

### Prerequisites
- Rust (latest stable)
- Z3 Theorem Prover (`brew install z3` on macOS)

### Installation
```bash
git clone https://github.com/mujthabamkdev/aether_store.git
cd aether_store
cargo build
```

### Usage

**1. Define your App**
Create a `guardian.yaml` manifest:
```yaml
app_name: "Personal Financial Guardian"
laws:
  - "no_riba"
  - "data_sovereignty"
nodes:
  - name: "shopee_analyzer"
    intent: "Calculate Zakat for 5000"
  - name: "root"
    intent: "Add 200 and 300"
    dependencies: ["shopee_analyzer"]
```

**2. Run the Factory**
This will:
- Parse the manifest.
- Weave intent into logic.
- Verify every atom against the Laws.
- Execute the root node.
- Start the Visualization Server.

```bash
cargo run
```

**3. Visualize**
Open `http://localhost:3000` to see your Logic Grid in real-time.

## 🧪 Verification

The system includes a self-verification routine in `src/main.rs` that demonstrates:
- **Build**: Compiling the manifest.
- **Execution**: Running the logic (Output: `500`).
- **Optimization**: Detecting slow execution (e.g., `53333ns`) and auto-optimizing.
- **Security**: Blocking a Foreign I/O request to `google.com` while allowing a Local I/O request.

## 📜 License
MIT License. Built for the future of AI-driven development.
