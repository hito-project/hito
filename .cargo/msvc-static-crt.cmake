# Builds C and C++ dependencies (the Manifold kernel, through CMake) against
# the static C runtime on Windows, to match `+crt-static` in config.toml.
# Without it they link the DLL runtime and the link fails. Other compilers
# ignore this setting.
set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded")
