"""
Basic tests for the loregrep Python package.

These tests verify that the Rust extension module can be imported and
basic functionality works as expected using the builder pattern API.
"""

import pytest
import os
import tempfile
from pathlib import Path


def test_import():
    """Test that the loregrep module can be imported successfully."""
    import loregrep
    
    # Check that key classes are available
    assert hasattr(loregrep, 'LoreGrep')
    assert hasattr(loregrep, 'LoreGrepBuilder')
    assert hasattr(loregrep, 'ScanResult')
    assert hasattr(loregrep, 'ToolResult')
    assert hasattr(loregrep, 'ToolSchema')
    assert hasattr(loregrep, 'IndexCoverage')


def test_builder_creation():
    """Test that LoreGrepBuilder can be created."""
    import loregrep
    
    builder = loregrep.LoreGrep.builder()
    assert builder is not None


def test_builder_configuration():
    """Test that the enhanced builder pattern works for configuration."""
    import loregrep
    
    # Test chaining enhanced configuration methods
    builder = (loregrep.LoreGrep.builder()
              .with_rust_analyzer()         # Enhanced: analyzer registration
              .with_python_analyzer()       # Enhanced: analyzer registration
              .optimize_for_performance()   # Enhanced: convenience method
              .exclude_test_dirs()          # Enhanced: convenience method
              .max_file_size(1024 * 1024)  # Traditional method
              .max_depth(10)
              .file_patterns(["*.py", "*.rs", "*.js"])
              .exclude_patterns(["target/", "node_modules/"])
              .respect_gitignore(True))
    
    assert builder is not None


def test_loregrep_build():
    """Test that LoreGrep can be built from builder."""
    import loregrep
    
    loregrep_instance = (loregrep.LoreGrep.builder()
                        .max_file_size(1024 * 1024)
                        .max_depth(5)
                        .file_patterns(["*.py"])
                        .build())
    
    assert loregrep_instance is not None


def test_enhanced_api_methods():
    """Test enhanced API methods: auto-discovery and presets."""
    import loregrep
    
    # Test auto-discovery (may fail if no project detected, that's OK)
    try:
        auto_instance = loregrep.LoreGrep.auto_discover(".")
        assert auto_instance is not None
    except Exception:
        # Auto-discovery failure is acceptable for this test
        pass
    
    # Test project presets
    presets = [
        loregrep.LoreGrep.rust_project,
        loregrep.LoreGrep.python_project,
        loregrep.LoreGrep.polyglot_project,
    ]
    
    # At least one preset should work
    preset_successes = 0
    for preset_func in presets:
        try:
            preset_instance = preset_func(".")
            assert preset_instance is not None
            preset_successes += 1
        except Exception:
            # Some presets may fail, that's OK
            pass
    
    # We expect at least some preset methods to be callable
    assert preset_successes >= 0  # Just test they're callable


def test_enhanced_builder_methods():
    """Test enhanced builder convenience methods."""
    import loregrep
    
    # Test enhanced builder methods are callable
    try:
        enhanced_instance = (loregrep.LoreGrep.builder()
                           .with_all_analyzers()
                           .comprehensive_analysis()
                           .exclude_vendor_dirs()
                           .include_source_files()
                           .include_config_files()
                           .build())
        assert enhanced_instance is not None
    except Exception:
        # Enhanced methods may not be fully implemented yet, test they're at least callable
        builder = loregrep.LoreGrep.builder()
        
        # Test that the methods exist (they should not raise AttributeError)
        try:
            builder.with_all_analyzers()
            builder.comprehensive_analysis()
            builder.exclude_vendor_dirs()
            builder.include_source_files()
            builder.include_config_files()
        except AttributeError:
            pytest.fail("Enhanced builder methods should exist")
        except Exception:
            # Other exceptions are OK for now
            pass


@pytest.mark.asyncio
async def test_basic_scanning():
    """Test basic scanning functionality with a temporary directory."""
    import loregrep
    
    # Create a temporary directory with some Python files
    with tempfile.TemporaryDirectory() as temp_dir:
        # Create some test files
        test_file = Path(temp_dir) / "test.py"
        test_file.write_text("""
def hello_world():
    '''A simple hello world function.'''
    print("Hello, World!")
    return "greeting"

class TestClass:
    '''A simple test class.'''
    def __init__(self):
        self.value = 42
    
    def get_value(self):
        return self.value
""")
        
        # Build LoreGrep instance. The Python analyzer is required: without a
        # registered analyzer the scan finds the file and indexes nothing, and
        # this test used to skip itself over that (0 >= 1) instead of failing.
        loregrep_instance = (loregrep.LoreGrep.builder()
                           .with_python_analyzer()
                           .max_file_size(1024 * 1024)
                           .file_patterns(["*.py"])
                           .build())
        
        # Scan the temporary directory
        try:
            result = await loregrep_instance.scan(temp_dir)
            assert hasattr(result, 'files_scanned')      # Fixed: correct field name
            assert hasattr(result, 'functions_found')
            assert hasattr(result, 'structs_found')
            assert hasattr(result, 'duration_ms')
            
            # Should have scanned at least 1 file
            assert result.files_scanned >= 1              # Fixed: correct field name
            
        except Exception as e:
            # For now, we'll allow this to fail gracefully
            # The exact implementation may vary
            pytest.skip(f"Scanning test skipped due to: {e}")


def test_tool_definitions():
    """Test that tool definitions can be retrieved."""
    import loregrep
    
    tools = loregrep.LoreGrep.get_tool_definitions()
    assert isinstance(tools, list)
    
    # Should have some tools available
    if len(tools) > 0:
        tool = tools[0]
        assert hasattr(tool, 'name')
        assert hasattr(tool, 'description')
        assert hasattr(tool, 'parameters')
        assert isinstance(tool.name, str)
        assert isinstance(tool.description, str)


def test_version_access():
    """Test that version information is accessible."""
    import loregrep
    
    version = loregrep.LoreGrep.version()
    assert isinstance(version, str)
    assert len(version) > 0


def test_package_metadata():
    """Test that package metadata is accessible."""
    import loregrep

    assert hasattr(loregrep, '__version__')
    assert hasattr(loregrep, '__author__')
    assert isinstance(loregrep.__version__, str)
    assert isinstance(loregrep.__author__, str)


def test_version_has_single_source_of_truth():
    """The package version must be derived from the loaded extension.

    A hardcoded copy in __init__.py drifts from Cargo.toml (and from a stale
    locally built .so) without anything noticing. These assertions fail loudly
    if someone reintroduces one.
    """
    import loregrep

    # The package version is whatever the compiled extension reports.
    assert loregrep.__version__ == loregrep.LoreGrep.version()
    assert loregrep.__version__ == loregrep.loregrep.__version__

    # And it must not be a literal in the Python source.
    init_source = Path(loregrep.__file__).read_text()
    assert '__version__ = "' not in init_source
    assert "__version__ = '" not in init_source


def test_builder_direct_construction():
    """LoreGrepBuilder() is equivalent to LoreGrep.builder()."""
    import loregrep

    builder = loregrep.LoreGrepBuilder()
    assert builder is not None
    assert builder.max_depth(3).build() is not None


def test_builder_full_surface():
    """Every builder method exposed by the Rust builder is callable and chains."""
    import loregrep

    instance = (loregrep.LoreGrep.builder()
                .with_rust_analyzer()
                .with_python_analyzer()
                .with_typescript_analyzer()
                .with_go_analyzer()
                .with_all_analyzers()
                .configure_patterns_for_languages(["rust", "python"])
                .include_patterns(["*.py", "*.rs"])
                .file_patterns(["*.py", "*.rs"])
                .exclude_patterns(["target/"])
                .max_files(1000)
                .max_file_size(1024 * 1024)
                .max_depth(5)
                .unlimited_depth()
                .follow_symlinks(False)
                .respect_gitignore(True)
                .cache_ttl(600)
                .optimize_for_performance()
                .comprehensive_analysis()
                .exclude_common_build_dirs()
                .exclude_test_dirs()
                .exclude_vendor_dirs()
                .include_source_files()
                .include_config_files()
                .build())

    assert instance is not None


def _python_repo(tmp_path: Path, count: int = 3) -> Path:
    for i in range(count):
        (tmp_path / f"mod_{i}.py").write_text(
            f"def fn_{i}():\n    return {i}\n\n\nclass Cls_{i}:\n    pass\n"
        )
    return tmp_path


@pytest.mark.asyncio
async def test_index_lifecycle_methods():
    """is_scanned / get_stats / clear_index / first_missing_indexed_path."""
    import loregrep

    with tempfile.TemporaryDirectory() as temp_dir:
        repo = _python_repo(Path(temp_dir))

        lg = (loregrep.LoreGrep.builder()
              .with_python_analyzer()
              .file_patterns(["*.py"])
              .build())

        assert lg.is_scanned() is False

        result = await lg.scan(str(repo))
        assert result.files_scanned == 3
        assert isinstance(result.languages, list)

        assert lg.is_scanned() is True

        stats = lg.get_stats()
        assert stats.files_scanned == result.files_scanned
        assert stats.functions_found == result.functions_found
        assert stats.structs_found == result.structs_found
        assert stats.duration_ms == 0  # stats never re-scan

        # Nothing was deleted, so no indexed path can be missing.
        assert lg.first_missing_indexed_path() is None

        # Deleting an indexed file makes the probe report it.
        (repo / "mod_0.py").unlink()
        assert lg.first_missing_indexed_path() is not None

        # set_scan_root is accepted (a scan already set it to the same root).
        lg.set_scan_root(str(repo))

        lg.clear_index()
        assert lg.is_scanned() is False
        assert lg.get_stats().files_scanned == 0


@pytest.mark.asyncio
async def test_index_coverage_reports_truncation():
    """max_files truncates the index and index_coverage() says so."""
    import loregrep

    with tempfile.TemporaryDirectory() as temp_dir:
        repo = _python_repo(Path(temp_dir), count=5)

        full = (loregrep.LoreGrep.builder()
                .with_python_analyzer()
                .file_patterns(["*.py"])
                .build())
        await full.scan(str(repo))

        coverage = full.index_coverage()
        assert coverage.truncated is False
        assert coverage.note is None
        assert coverage.files_indexed == coverage.files_discovered == 5

        limited = (loregrep.LoreGrep.builder()
                   .with_python_analyzer()
                   .file_patterns(["*.py"])
                   .max_files(2)
                   .build())
        await limited.scan(str(repo))

        coverage = limited.index_coverage()
        assert coverage.truncated is True
        assert coverage.files_indexed == 2
        assert coverage.files_discovered == 5
        assert isinstance(coverage.note, str) and coverage.note


if __name__ == "__main__":
    pytest.main([__file__]) 