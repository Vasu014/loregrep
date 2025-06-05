#!/usr/bin/env python3
"""
Enhanced Builder Example - Convenience Methods with Real-Time Feedback

This example demonstrates the enhanced builder pattern with:
- Language analyzer registration with feedback
- Convenience methods for common configurations
- Real-time validation and configuration summaries

Perfect for when you want control with convenience!
"""

import asyncio
import loregrep


async def main():
    print("🛠️  LoreGrep Enhanced Builder Example")
    print("=" * 45)
    
    try:
        print("Creating LoreGrep instance with enhanced builder...")
        
        # Enhanced builder with convenience methods and real-time feedback
        loregrep_instance = (loregrep.LoreGrep.builder()
                           .with_rust_analyzer()         # ✅ Real-time feedback
                           .with_python_analyzer()       # ✅ Registration confirmation
                           .optimize_for_performance()   # 🚀 Speed-optimized preset
                           .exclude_test_dirs()          # 🚫 Skip test directories
                           .exclude_vendor_dirs()        # 🚫 Skip vendor/dependencies
                           .max_file_size(2 * 1024 * 1024)  # 2MB file limit
                           .build())                     # 🎆 Configuration summary
        
        print("✅ Enhanced builder instance created successfully!")
        
        # Create a sample directory for demonstration
        print("\n📁 Creating sample Python project...")
        sample_dir = create_sample_python_project()
        
        try:
            # Scan the sample project
            print(f"\n🔍 Scanning sample project at {sample_dir}...")
            result = await loregrep_instance.scan(sample_dir)
            
            print(f"✅ Enhanced scan completed!")
            print(f"   📁 Files scanned: {result.files_scanned}")
            print(f"   🔧 Functions found: {result.functions_found}")
            print(f"   📦 Structs/Classes found: {result.structs_found}")
            print(f"   ⏱️  Duration: {result.duration_ms}ms")
            
            # Demonstrate enhanced tool execution
            print("\n🔍 Searching for 'config' related functions...")
            func_result = await loregrep_instance.execute_tool("search_functions", {
                "pattern": "config",
                "limit": 10
            })
            print("✅ Function search completed!")
            
            print("\n🌳 Getting repository structure...")
            tree_result = await loregrep_instance.execute_tool("get_repository_tree", {
                "include_file_details": True,
                "max_depth": 3
            })
            print("✅ Repository tree generated!")
            
            print("\n🎉 Enhanced builder example completed successfully!")
            print("💡 The enhanced builder gives you:")
            print("   ✅ Real-time feedback during configuration")
            print("   🚀 Convenient preset methods")
            print("   🎛️  Full control over settings")
            
        finally:
            cleanup_sample_project(sample_dir)
            print("\n🧹 Sample project cleaned up")
            
    except Exception as e:
        print(f"❌ Enhanced builder example failed: {e}")
        print("💡 Make sure LoreGrep bindings are installed:")
        print("   maturin develop --features python")


def create_sample_python_project():
    """Create a temporary Python project for demonstration."""
    import tempfile
    from pathlib import Path
    
    temp_dir = tempfile.mkdtemp(prefix="loregrep_enhanced_")
    base_path = Path(temp_dir)
    
    # Main application file
    (base_path / "app.py").write_text('''
"""Main application module."""

import os
import sys
from typing import List, Dict, Optional

class ConfigManager:
    """Manages application configuration."""
    
    def __init__(self, config_file: str = "config.json"):
        self.config_file = config_file
        self.settings: Dict[str, any] = {}
        
    def load_config(self) -> Dict[str, any]:
        """Load configuration from file."""
        try:
            with open(self.config_file, 'r') as f:
                import json
                self.settings = json.load(f)
                return self.settings
        except FileNotFoundError:
            return self.get_default_config()
    
    def get_default_config(self) -> Dict[str, any]:
        """Get default configuration values."""
        return {
            "database_url": "sqlite:///app.db",
            "debug": False,
            "port": 8000,
            "host": "localhost"
        }
    
    def save_config(self, settings: Dict[str, any]) -> bool:
        """Save configuration to file."""
        try:
            import json
            with open(self.config_file, 'w') as f:
                json.dump(settings, f, indent=2)
            return True
        except Exception:
            return False

def initialize_app() -> ConfigManager:
    """Initialize the application with configuration."""
    config = ConfigManager()
    config.load_config()
    return config

def main():
    """Main application entry point."""
    config = initialize_app()
    print(f"App started with config: {config.settings}")

if __name__ == "__main__":
    main()
''')
    
    # Utils module
    (base_path / "utils.py").write_text('''
"""Utility functions for the application."""

import hashlib
import datetime
from typing import List, Any


def hash_password(password: str) -> str:
    """Hash a password using SHA-256."""
    return hashlib.sha256(password.encode()).hexdigest()


def validate_email(email: str) -> bool:
    """Simple email validation."""
    return "@" in email and "." in email.split("@")[1]


def format_timestamp(timestamp: float) -> str:
    """Format a timestamp to human-readable string."""
    dt = datetime.datetime.fromtimestamp(timestamp)
    return dt.strftime("%Y-%m-%d %H:%M:%S")


class Logger:
    """Simple logging utility."""
    
    def __init__(self, name: str):
        self.name = name
        
    def info(self, message: str) -> None:
        """Log an info message."""
        timestamp = format_timestamp(datetime.datetime.now().timestamp())
        print(f"[{timestamp}] INFO [{self.name}]: {message}")
        
    def error(self, message: str) -> None:
        """Log an error message."""
        timestamp = format_timestamp(datetime.datetime.now().timestamp())
        print(f"[{timestamp}] ERROR [{self.name}]: {message}")


def process_batch(items: List[Any], batch_size: int = 100) -> List[List[Any]]:
    """Split items into batches."""
    batches = []
    for i in range(0, len(items), batch_size):
        batches.append(items[i:i + batch_size])
    return batches
''')
    
    # Database module
    (base_path / "database.py").write_text('''
"""Database operations module."""

import sqlite3
from typing import List, Dict, Optional, Any
from utils import Logger


class DatabaseConnection:
    """Simple database connection wrapper."""
    
    def __init__(self, db_path: str):
        self.db_path = db_path
        self.logger = Logger("Database")
        
    def connect(self) -> sqlite3.Connection:
        """Create database connection."""
        self.logger.info(f"Connecting to database: {self.db_path}")
        return sqlite3.connect(self.db_path)
        
    def execute_query(self, query: str, params: tuple = ()) -> List[Dict[str, Any]]:
        """Execute a SELECT query and return results."""
        with self.connect() as conn:
            conn.row_factory = sqlite3.Row
            cursor = conn.cursor()
            cursor.execute(query, params)
            return [dict(row) for row in cursor.fetchall()]
    
    def execute_update(self, query: str, params: tuple = ()) -> int:
        """Execute an INSERT, UPDATE, or DELETE query."""
        with self.connect() as conn:
            cursor = conn.cursor()
            cursor.execute(query, params)
            conn.commit()
            return cursor.rowcount


class UserRepository:
    """Repository for user operations."""
    
    def __init__(self, db: DatabaseConnection):
        self.db = db
        self.logger = Logger("UserRepository")
        
    def create_user(self, name: str, email: str, password_hash: str) -> int:
        """Create a new user."""
        query = "INSERT INTO users (name, email, password_hash) VALUES (?, ?, ?)"
        return self.db.execute_update(query, (name, email, password_hash))
        
    def get_user_by_email(self, email: str) -> Optional[Dict[str, Any]]:
        """Get user by email address."""
        query = "SELECT * FROM users WHERE email = ?"
        results = self.db.execute_query(query, (email,))
        return results[0] if results else None
        
    def list_users(self, limit: int = 100) -> List[Dict[str, Any]]:
        """List all users with optional limit."""
        query = "SELECT id, name, email FROM users LIMIT ?"
        return self.db.execute_query(query, (limit,))
''')
    
    return temp_dir


def cleanup_sample_project(temp_dir: str):
    """Clean up the temporary sample project."""
    import shutil
    try:
        shutil.rmtree(temp_dir)
    except OSError:
        pass


if __name__ == "__main__":
    asyncio.run(main())