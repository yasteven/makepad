package dev.makepad.android;

import android.app.Activity;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.net.ConnectivityManager;
import android.net.LinkAddress;
import android.net.LinkProperties;
import android.net.Network;
import android.net.NetworkCapabilities;
import android.net.RouteInfo;
import org.json.JSONArray;
import org.json.JSONObject;

/** Foreground device services. Callbacks contain a request ID, never log text. */
final class MakepadDeviceServices {
    private static final int TEXT_LIMIT = 65536;
    private static final int NETWORK_LIMIT = 131072;

    static void perform(Activity activity, long requestId, int operation, String text) {
        try {
            if (activity.isDestroyed() || activity.isFinishing()) {
                throw new IllegalStateException("Activity is closing; request cancelled");
            }
            if (operation == 2) {
                String snapshot = networks(activity);
                MakepadNative.onDeviceServiceResult(requestId, operation, true, snapshot);
                return;
            }
            if (!activity.hasWindowFocus()) {
                throw new IllegalStateException("Clipboard requires a focused foreground app");
            }
            ClipboardManager manager = (ClipboardManager) activity.getSystemService(Context.CLIPBOARD_SERVICE);
            if (manager == null) throw new IllegalStateException("Clipboard service unavailable");
            if (operation == 0) {
                if (text == null || text.length() > TEXT_LIMIT) {
                    throw new IllegalArgumentException("Clipboard text exceeds its size limit");
                }
                manager.setPrimaryClip(ClipData.newPlainText("Text", text));
                MakepadNative.onDeviceServiceResult(requestId, operation, true, "");
            } else if (operation == 1) {
                ClipData clip = manager.getPrimaryClip();
                if (clip == null || clip.getItemCount() == 0) {
                    throw new IllegalStateException("Clipboard is empty or Android denied access");
                }
                // Do not dereference arbitrary content URIs or perform provider I/O.
                CharSequence value = clip.getItemAt(0).getText();
                if (value == null) throw new IllegalStateException("Clipboard has no plain text");
                if (value.length() > TEXT_LIMIT) throw new IllegalStateException("Clipboard text exceeds its size limit");
                MakepadNative.onDeviceServiceResult(requestId, operation, true, value.toString());
            } else {
                throw new IllegalArgumentException("Unknown device-service operation");
            }
        } catch (Exception error) {
            // Exception text can contain provider/user content. Return fixed messages.
            String message = error instanceof SecurityException
                ? "Android denied device-service permission"
                : "Device service unavailable, cancelled, unfocused, or result too large";
            MakepadNative.onDeviceServiceResult(requestId, operation, false, message);
        }
    }

    private static String networks(Activity activity) throws Exception {
        ConnectivityManager manager = (ConnectivityManager) activity.getSystemService(Context.CONNECTIVITY_SERVICE);
        if (manager == null) throw new IllegalStateException("Connectivity service unavailable");
        Network active = manager.getActiveNetwork();
        Network[] networks = manager.getAllNetworks();
        if (networks.length > 64) throw new IllegalStateException("Too many networks");
        JSONArray result = new JSONArray();
        for (Network network : networks) {
            LinkProperties link = manager.getLinkProperties(network);
            NetworkCapabilities caps = manager.getNetworkCapabilities(network);
            // Networks may disappear between the two framework calls.
            if (link == null || caps == null) continue;
            JSONObject item = new JSONObject();
            item.put("handle", Long.toUnsignedString(network.getNetworkHandle()));
            item.put("interface", link.getInterfaceName() == null ? "" : link.getInterfaceName());
            item.put("default", network.equals(active));
            item.put("internet", caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET));
            item.put("validated", caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED));
            JSONArray transports = new JSONArray();
            if (caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET)) transports.put("ethernet");
            if (caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI)) transports.put("wifi");
            if (caps.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR)) transports.put("cellular");
            if (caps.hasTransport(NetworkCapabilities.TRANSPORT_USB)) transports.put("usb");
            if (caps.hasTransport(NetworkCapabilities.TRANSPORT_VPN)) transports.put("vpn");
            if (caps.hasTransport(NetworkCapabilities.TRANSPORT_BLUETOOTH)) transports.put("bluetooth");
            item.put("transports", transports);
            if (link.getLinkAddresses().size() > 64 || link.getRoutes().size() > 128) {
                throw new IllegalStateException("Network details exceed limits");
            }
            JSONArray addresses = new JSONArray();
            for (LinkAddress address : link.getLinkAddresses()) addresses.put(address.toString());
            item.put("addresses", addresses);
            JSONArray routes = new JSONArray();
            for (RouteInfo route : link.getRoutes()) routes.put(route.toString());
            item.put("routes", routes);
            result.put(item);
        }
        String json = result.toString();
        if (json.length() > NETWORK_LIMIT) throw new IllegalStateException("Network snapshot exceeds limit");
        return json;
    }
}
