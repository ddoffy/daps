# D. AWS Parameter Store Tab Completion
A command-line tool that provides tab completion for AWS Parameter Store paths, making it easier to navigate and use AWS SSM parameters in your terminal.


# Overview
`daps` is a lightweight CLI tool that enhances your workflow when working with AWS Systems Manager Parameter Store. It provides intelligent tab completion for parameter paths, allowing you to quickly navigate the parameter hierarchy without having to remember or manually type full parameter paths.
Features

Tab completion for AWS SSM parameter paths
Support for multiple AWS profiles and regions
Fast parameter lookup with local caching
Easy integration with bash, zsh, and fish shells
Lightweight with minimal dependencies
Cross-platform support (Linux, Windows) - I am too lazy to open my ARM MacBook; if you have a Mac (x86_64/ARM) and are interested in this project, please consider contributing.  ^^, 

# Installation
Pre-built Binaries
Download the latest release for your platform:

- [Linux (x86_64)](https://github.com/ddoffy/daps/releases)
- [Windows (x86_64)](https://github.com/ddoffy/daps/releases) 

### Linux/macOs
Make the binary executable (Linux/macOS):
```
chmod +x ./daps
```

Move to a directory in your PATH:
bash# Linux/macOS

```
sudo mv ./daps /usr/local/bin/daps
```

### Windows
> TODO

# Usage
## Basic Usage
Once shell integration is set up, you can use tab completion with the AWS CLI or directly with the tool:
bash# With AWS CLI
```
daps --path /<prefix of yours>
```

After it loaded all your parameters by path, you can you tab tab tab completion. 

You can reload cache the paramater by typing `reload`, it will automatically reload the selected path.

If you wanna set new value, please typing `set <new value>`, It will update new value for the selected path.

If you wanna insert a new parameter stored, please typing `insert <path>:<value>:<param type>` to insert new parameter stored

Typing `exit` or `ctrl+D` or `ctrl+C` to quit.

# Configuration
The tool uses your standard AWS configuration from ~/.aws/config and ~/.aws/credentials with default profile.

# Contributing
Contributions are welcome! Please see CONTRIBUTING.md for details.

# License
This project is licensed under the MIT License - see the LICENSE file for details.
