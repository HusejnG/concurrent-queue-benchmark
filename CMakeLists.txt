cmake_minimum_required(VERSION 3.16)
project(concurrent_queue_benchmark CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

if(NOT CMAKE_BUILD_TYPE)
  set(CMAKE_BUILD_TYPE Release)
endif()

find_package(Threads REQUIRED)

# ---------------------------------------------------------------------------
# Main benchmark executable
# ---------------------------------------------------------------------------
add_executable(pc_benchmark src/main.cpp)
target_include_directories(pc_benchmark PRIVATE include)
target_link_libraries(pc_benchmark PRIVATE Threads::Threads)

if(MSVC)
  target_compile_options(pc_benchmark PRIVATE /W4)
else()
  target_compile_options(pc_benchmark PRIVATE -Wall -Wextra -O2)
endif()

# ---------------------------------------------------------------------------
# Tests (Google Test, fetched automatically)
# ---------------------------------------------------------------------------
option(BUILD_TESTS "Build unit tests" ON)

if(BUILD_TESTS)
  include(FetchContent)
  FetchContent_Declare(
    googletest
    GIT_REPOSITORY https://github.com/google/googletest.git
    GIT_TAG        v1.15.2
  )
  # Keep gtest's own runtime/library settings consistent with this project on Windows.
  set(gtest_force_shared_crt ON CACHE BOOL "" FORCE)
  FetchContent_MakeAvailable(googletest)

  enable_testing()

  add_executable(pc_tests tests/test_queues.cpp)
  target_include_directories(pc_tests PRIVATE include)
  target_link_libraries(pc_tests PRIVATE gtest_main Threads::Threads)

  include(GoogleTest)
  gtest_discover_tests(pc_tests)
endif()
