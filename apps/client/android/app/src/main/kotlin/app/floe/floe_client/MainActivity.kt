package app.floe.floe_client

import android.content.Intent
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.embedding.android.FlutterActivity

class MainActivity : FlutterActivity() {
    private var contextChannel: AndroidContextChannel? = null

    private external fun nativeConfigureVault(): Boolean

    companion object {
        init {
            System.loadLibrary("floe_ffi")
        }
    }

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        check(nativeConfigureVault()) { "Floe vault key storage is unavailable" }
        contextChannel = AndroidContextChannel(this, flutterEngine.dartExecutor.binaryMessenger)
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        if (contextChannel?.onActivityResult(requestCode, resultCode, data) == true) return
        super.onActivityResult(requestCode, resultCode, data)
    }

    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray,
    ) {
        if (contextChannel?.onRequestPermissionsResult(requestCode, grantResults) == true) return
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
    }

    override fun cleanUpFlutterEngine(flutterEngine: FlutterEngine) {
        contextChannel?.close()
        contextChannel = null
        super.cleanUpFlutterEngine(flutterEngine)
    }
}
