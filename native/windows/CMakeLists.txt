# Builds the DirectShow camera filter (dji_virtual_camera.dll).
#
# The DirectShow base classes are not part of the modern Windows SDK, so the
# MIT-licensed Microsoft sources are vendored under baseclasses/.

cmake_minimum_required(VERSION 3.20)
project(dji_virtual_camera CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>")

file(GLOB BASECLASSES_SOURCES "${CMAKE_CURRENT_SOURCE_DIR}/baseclasses/*.cpp")
add_library(strmbase STATIC ${BASECLASSES_SOURCES})
target_include_directories(strmbase PUBLIC "${CMAKE_CURRENT_SOURCE_DIR}/baseclasses")
target_compile_definitions(strmbase PUBLIC UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
# The 2001-era sources trip newer warnings; they are not ours to fix.
target_compile_options(strmbase PRIVATE /w /wd4996)

add_library(dji_virtual_camera SHARED
    virtual_camera.cpp
    virtual_camera.def
)
target_link_libraries(dji_virtual_camera PRIVATE strmbase strmiids winmm ole32 oleaut32 uuid advapi32 user32 gdi32)
target_include_directories(dji_virtual_camera PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
target_compile_definitions(dji_virtual_camera PRIVATE UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
set_target_properties(dji_virtual_camera PROPERTIES OUTPUT_NAME "dji_virtual_camera")
