package com.github.alexmoon.bluest.proxy.android.bluetooth.le;

@SuppressWarnings("rawtypes")
class ScanCallback extends android.bluetooth.le.ScanCallback {
    long ptr;

    private ScanCallback(long ptr) {
        this.ptr = ptr;
    }

    @Override
    protected void finalize() throws Throwable {
        native_finalize(this.ptr);
    }
    private native void native_finalize(long ptr);

    @Override
    public void onScanResult(int arg0, android.bluetooth.le.ScanResult arg1) {
        native_onScanResult(ptr, arg0, arg1);
    }
    private native void native_onScanResult(long ptr, int arg0, android.bluetooth.le.ScanResult arg1);

    @Override
    public void onBatchScanResults(java.util.List arg0) {
        native_onBatchScanResults(ptr, arg0);
    }
    private native void native_onBatchScanResults(long ptr, java.util.List arg0);

    @Override
    public void onScanFailed(int arg0) {
        native_onScanFailed(ptr, arg0);
    }
    private native void native_onScanFailed(long ptr, int arg0);

}
