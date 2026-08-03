# Dollrunner

Dollrunner is a cross-platform desktop application built for save file mapping and visualizer updates. Engineered for high performance and a rich, interactive user experience, Dollrunner features a custom draggable titlebar, a frameless window layout, and a backend powered by Rust and C#.

## 🚀 Architecture

Dollrunner is built on a modern, high-performance tech stack:

- **Frontend**: Svelte 5 with Vite. Features a dynamic dashboard, interactive 3D model visualization, and responsive design tailored for desktop environments.
- **Backend / Container**: Tauri v2 (Rust). Provides a secure, lightweight, and native application runtime with custom window controls and a frameless UI.
- **Sidecar Process**: C# (.NET). A self-contained executable sidecar handles complex save decryption, extraction, and validation logic natively.

## 🛠 Features

- **Custom Window Controls**: A borderless window with a draggable monochrome titlebar.
- **Dashboard Interface**: Rich Trainer card statistics, dynamic gym badge SVGs with grayscale active/inactive state handling, and quick macro access.
- **Interactive Visualizer**: 3D model inspection of Party and PC box items.
- **Robust Save Processing**: C# sidecar integration to ensure secure, non-destructive data manipulation, avoiding OS execution errors.

## 📦 Requirements

To build or run Dollrunner from source, you must have the following installed:
- [Node.js](https://nodejs.org/) (v18 or higher)
- [Rust](https://www.rust-lang.org/tools/install) (latest stable)
- [C# / .NET SDK](https://dotnet.microsoft.com/download) (for modifying the sidecar process)

## 🏃 Getting Started

### 1. Install Dependencies
Run the following command to install the necessary frontend packages:
```bash
npm install
```

### 2. Development Mode
To start the Svelte development server and the Tauri window simultaneously with hot-module replacement (HMR):
```bash
npm run tauri dev
```

### 3. Production Build
To compile the frontend assets, compile the Rust backend, and bundle the final `.exe` (or your OS's respective installation package):
```bash
npm run tauri build
```
The compiled binaries will be placed in `src-tauri/target/release/bundle/`.

## 🔒 Security

All secrets and keys must be handled via environment variables and are completely omitted from the repository. A complete history audit confirms zero leakages of API keys, tokens, or credentials within this repository.

## 📄 License
This project is licensed under the MIT License.
