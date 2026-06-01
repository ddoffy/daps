# WARP.md

This file provides guidance to WARP (warp.dev) when working with code in this repository.

## Project Overview

DAPS is a command-line tool that provides tab completion for AWS Parameter Store paths, making it easier to navigate and use AWS SSM parameters in your terminal. It's built in Rust using Tokio for async operations and provides an interactive CLI with parameter caching and encryption capabilities.

## Common Development Commands

### Build and Run
- `cargo build --release` - Build optimized release version
- `cargo build --verbose --release` - Build with verbose output (matches CI)
- `cargo run -- --help` - Show command-line options
- `cargo run -- --path /dev/ --verbose` - Run with specific parameter path and verbose logging
- `cargo install --path .` - Install locally

### Development Tools
- `cargo check` - Fast syntax and type checking
- `cargo clippy` - Linting (no custom configuration)
- `cargo fmt` - Code formatting (no custom configuration)

### Testing
Note: This project currently has no test suite. Tests would need to be added for proper development.

### Environment Setup
The application uses environment variables:
- `DAPS_ENCRYPTION_KEY` - Optional encryption key for parameter values (defaults to "default_key")
- `AWS_PROFILE` - AWS profile to use (standard AWS SDK behavior)
- `AWS_REGION` - AWS region (can also be set via `--region` flag)

## Architecture Overview

### Core Components

**Main Application (`src/main.rs`)**
- Entry point with CLI argument parsing using StructOpt
- Interactive terminal interface using rustyline with tab completion
- Command processing loop with colored output
- Clipboard integration for parameter values

**Parameter Completer System**
- `ParameterCompleter` - Core logic for AWS SSM parameter fetching and caching
- `ParamStoreHelper` - Rustyline integration providing completion, highlighting, and validation
- Hierarchical parameter path completion with command support

**Encryption Module (`src/encryption.rs`)**
- AES-256-GCM encryption for local parameter value storage
- SHA-256 key derivation from environment variable
- Base64 encoding for encrypted data storage

### Key Data Structures

- `parameters: HashMap<String, Vec<String>>` - Hierarchical path completion cache
- `values: HashMap<String, String>` - Parameter name to decrypted value mapping
- `metadata: HashMap<String, String>` - Session metadata (selected parameter, etc.)

### File Storage System
Parameters and values are cached locally in the `parameters/` directory (or custom via `--store-dir`):
- `parameters_{base_path}.txt` - Parameter paths for completion
- `values_{base_path}.txt` - Encrypted parameter values

### AWS Integration
- Uses Rusoto SDK (v0.47) for AWS SSM operations
- Supports pagination for large parameter sets
- Handles both encrypted and non-encrypted parameters
- Implements retry logic and error handling

## Command System

The interactive CLI supports these commands:
- `set <value>` - Update selected parameter value
- `insert <path>:<value>:<type>` - Create new parameter
- `search <term>` - Search parameters by name
- `select/sel <index>` - Select parameter from search results
- `reload` - Refresh single parameter from AWS
- `reload-by-path <path>` - Reload specific parameter
- `reload-by-paths <path>` - Reload all parameters under path
- `refresh` - Clear cache and reload all parameters
- `migration` - Migrate old format cached values to encrypted format
- `exit` - Quit application

## Development Notes

### Dependencies
- **Runtime**: Tokio async runtime with rusoto for AWS, rustyline for CLI
- **Crypto**: AES-GCM encryption with SHA-256 key derivation
- **UI**: Colored terminal output, clipboard integration, Vi-mode editing

### Cross-Platform Considerations
- File path handling differs between Windows (`\`) and Unix (`/`)
- Home directory detection uses `APPDATA` on Windows, `HOME` on Unix
- Clipboard integration may have platform-specific requirements

### Performance Optimization
- Local parameter caching to avoid repeated AWS calls
- Pagination handling for large parameter hierarchies
- Lazy loading with refresh-on-demand

### Error Handling
- Comprehensive AWS error handling with user-friendly messages
- File I/O error handling for cache operations
- Encryption/decryption error recovery

## Code Architecture Patterns

### Async/Await Pattern
All AWS operations use async/await with proper error propagation. The main loop is synchronous but spawns async operations for AWS calls.

### Shared State Management
Uses `Arc<Mutex<T>>` for thread-safe shared state between the completer and CLI interface components.

### Command Pattern
Interactive commands are parsed and dispatched through a match statement with async handlers for each operation type.

### Builder Pattern
Configuration uses StructOpt for CLI argument parsing with sensible defaults and validation.
