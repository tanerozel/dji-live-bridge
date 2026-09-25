# Keep app-specific rules here when the release pipeline needs them.
# JNI exports use this exact class and method name.
-keep class com.streammydrone.app.NativeRelay {
    native <methods>;
}
