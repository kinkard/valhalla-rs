cmake_minimum_required(VERSION 3.20 FATAL_ERROR)
project(libvalhalla-sys LANGUAGES CXX C)

# Compiler configuration
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

# Enable link-time optimization only in Release configuration to have reasonable compile times in Debug
if(CMAKE_BUILD_TYPE STREQUAL "Release")
  set(CMAKE_INTERPROCEDURAL_OPTIMIZATION TRUE)
endif()

include(CheckCXXCompilerFlag)

# Clang warning documentation - https://clang.llvm.org/docs/DiagnosticsReference.html
# GCC warning documentation - https://gcc.gnu.org/onlinedocs/gcc/Warning-Options.html
set(COMPILE_FLAGS -Werror -Wall -Wextra -pedantic -pedantic-errors)
set(EXTRA_COMPILE_FLAGS
  -Wcast-align
  -Wcast-qual
  -Wcomma
  -Wduplicated-cond
  -Wextra-semi
  -Wfloat-equal
  -Wlogical-op
  -Wnewline-eof
  -Wno-dangling-else
  -Wnon-virtual-dtor
  -Woverloaded-virtual
  -Wredundant-decls
  -Wsuggest-override
  -Wunreachable-code
  -Wconversion
)
foreach(extra_flag ${EXTRA_COMPILE_FLAGS})
  check_cxx_compiler_flag(${extra_flag} HAS_FLAG_${extra_flag})
  if(HAS_FLAG_${extra_flag})
      set(COMPILE_FLAGS ${COMPILE_FLAGS} ${extra_flag})
  endif()
endforeach()

# Disable everything we don't need to reduce number of system dependencies and speed up compilation
set(ENABLE_TOOLS OFF CACHE BOOL "disable Valhalla tools")
set(ENABLE_DATA_TOOLS OFF CACHE BOOL "disable Valhalla data tools")
set(ENABLE_SERVICES OFF CACHE BOOL "disable Valhalla services")
set(ENABLE_HTTP OFF CACHE BOOL "disable Valhalla http functionality")
set(ENABLE_PYTHON_BINDINGS OFF CACHE BOOL "disable Valhalla python bindings")
set(ENABLE_TESTS OFF CACHE BOOL "disable Valhalla tests")
set(ENABLE_GDAL OFF CACHE BOOL "disable Valhalla gdal usage")
add_subdirectory(valhalla EXCLUDE_FROM_ALL)

add_library(libvalhalla-sys src/libvalhalla.cpp)
target_link_libraries(libvalhalla-sys valhalla)
