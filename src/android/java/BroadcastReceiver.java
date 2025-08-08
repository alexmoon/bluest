package com.github.alexmoon.bluest.proxy.android.content;

@SuppressWarnings("rawtypes")
class BroadcastReceiver extends android.content.BroadcastReceiver {
    long ptr;

    private BroadcastReceiver(long ptr) {
        this.ptr = ptr;
    }

    @Override
    protected void finalize() throws Throwable {
        native_finalize(this.ptr);
    }
    private native void native_finalize(long ptr);

    @Override
    public void onReceive(android.content.Context arg0, android.content.Intent arg1) {
        native_onReceive(ptr, arg0, arg1);
    }
    private native void native_onReceive(long ptr, android.content.Context arg0, android.content.Intent arg1);
}
