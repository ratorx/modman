# Modman
Modman is a tool for managing dotfiles. Currently it supports:
* Symlinking local files into system locations
* Init and Cleanup scripts

# Usage
Modman has 3 commands:
* list - List all available modules
* install - Install the specified modules. This has 3 phases:
    * Verify that user has access to all the system locations
    * Symlink the files required
    * Run an optional init script
* uninstall - Uninstalls the specified modules. This has 3 phases:
    * Verify that user has access to the system locations and the files are symlinks to module files
    * Delete the symlinks
    * Run an optional cleanup script

# Improvements over modman 1.0
* Better checking to make sure module is valid
* Better error messages
* Rust > Python

# Planned features
* Dependency management
    * Handle system package depednencies
    * Handle module dependencies
* Host-specific configuration
    * Host specific resources
    * Host specific dependencies
* Atomic install/uninstall