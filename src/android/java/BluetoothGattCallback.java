package com.github.alexmoon.bluest.proxy.android.bluetooth;

@SuppressWarnings("rawtypes")
class BluetoothGattCallback extends android.bluetooth.BluetoothGattCallback {
    long ptr;

    private BluetoothGattCallback(long ptr) {
        this.ptr = ptr;
    }

    @Override
    protected void finalize() throws Throwable {
        native_finalize(this.ptr);
    }
    private native void native_finalize(long ptr);

    @Override
    public void onPhyUpdate(android.bluetooth.BluetoothGatt arg0, int arg1, int arg2, int arg3) {
        native_onPhyUpdate(ptr, arg0, arg1, arg2, arg3);
    }
    private native void native_onPhyUpdate(long ptr, android.bluetooth.BluetoothGatt arg0, int arg1, int arg2, int arg3);

    @Override
    public void onPhyRead(android.bluetooth.BluetoothGatt arg0, int arg1, int arg2, int arg3) {
        native_onPhyRead(ptr, arg0, arg1, arg2, arg3);
    }
    private native void native_onPhyRead(long ptr, android.bluetooth.BluetoothGatt arg0, int arg1, int arg2, int arg3);

    @Override
    public void onConnectionStateChange(android.bluetooth.BluetoothGatt arg0, int arg1, int arg2) {
        native_onConnectionStateChange(ptr, arg0, arg1, arg2);
    }
    private native void native_onConnectionStateChange(long ptr, android.bluetooth.BluetoothGatt arg0, int arg1, int arg2);

    @Override
    public void onServicesDiscovered(android.bluetooth.BluetoothGatt arg0, int arg1) {
        native_onServicesDiscovered(ptr, arg0, arg1);
    }
    private native void native_onServicesDiscovered(long ptr, android.bluetooth.BluetoothGatt arg0, int arg1);

    // NOTE: these `if (android.os.Build.VERSION.SDK_INT >= 33) { return; }` may be removed while updating `java-spaghetti`.
    // This is a temporary optimization, hopefully making up for the performance costs of workarounds for the current `java-spaghetti`.
    // The Rust code cannot rely on this behavior.

    @Override
    public void onCharacteristicRead(android.bluetooth.BluetoothGatt arg0, android.bluetooth.BluetoothGattCharacteristic arg1, int arg2) {
        if (android.os.Build.VERSION.SDK_INT >= 33) {
            return;
        }
        native_onCharacteristicRead(ptr, arg0, arg1, arg2);
    }
    private native void native_onCharacteristicRead(long ptr, android.bluetooth.BluetoothGatt arg0, android.bluetooth.BluetoothGattCharacteristic arg1, int arg2);

    @Override
    public void onCharacteristicRead(android.bluetooth.BluetoothGatt arg0, android.bluetooth.BluetoothGattCharacteristic arg1, byte[] arg2, int arg3) {
        native_onCharacteristicRead(ptr, arg0, arg1, arg2, arg3);
    }
    private native void native_onCharacteristicRead(long ptr, android.bluetooth.BluetoothGatt arg0, android.bluetooth.BluetoothGattCharacteristic arg1, byte[] arg2, int arg3);

    @Override
    public void onCharacteristicWrite(android.bluetooth.BluetoothGatt arg0, android.bluetooth.BluetoothGattCharacteristic arg1, int arg2) {
        native_onCharacteristicWrite(ptr, arg0, arg1, arg2);
    }
    private native void native_onCharacteristicWrite(long ptr, android.bluetooth.BluetoothGatt arg0, android.bluetooth.BluetoothGattCharacteristic arg1, int arg2);

    @Override
    public void onCharacteristicChanged(android.bluetooth.BluetoothGatt arg0, android.bluetooth.BluetoothGattCharacteristic arg1) {
        if (android.os.Build.VERSION.SDK_INT >= 33) {
            return;
        }
        native_onCharacteristicChanged(ptr, arg0, arg1);
    }
    private native void native_onCharacteristicChanged(long ptr, android.bluetooth.BluetoothGatt arg0, android.bluetooth.BluetoothGattCharacteristic arg1);

    @Override
    public void onCharacteristicChanged(android.bluetooth.BluetoothGatt arg0, android.bluetooth.BluetoothGattCharacteristic arg1, byte[] arg2) {
        native_onCharacteristicChanged(ptr, arg0, arg1, arg2);
    }
    private native void native_onCharacteristicChanged(long ptr, android.bluetooth.BluetoothGatt arg0, android.bluetooth.BluetoothGattCharacteristic arg1, byte[] arg2);

    @Override
    public void onDescriptorRead(android.bluetooth.BluetoothGatt arg0, android.bluetooth.BluetoothGattDescriptor arg1, int arg2) {
        if (android.os.Build.VERSION.SDK_INT >= 33) {
            return;
        }
        native_onDescriptorRead(ptr, arg0, arg1, arg2);
    }
    private native void native_onDescriptorRead(long ptr, android.bluetooth.BluetoothGatt arg0, android.bluetooth.BluetoothGattDescriptor arg1, int arg2);

    @Override
    public void onDescriptorRead(android.bluetooth.BluetoothGatt arg0, android.bluetooth.BluetoothGattDescriptor arg1, int arg2, byte[] arg3) {
        native_onDescriptorRead(ptr, arg0, arg1, arg2, arg3);
    }
    private native void native_onDescriptorRead(long ptr, android.bluetooth.BluetoothGatt arg0, android.bluetooth.BluetoothGattDescriptor arg1, int arg2, byte[] arg3);

    @Override
    public void onDescriptorWrite(android.bluetooth.BluetoothGatt arg0, android.bluetooth.BluetoothGattDescriptor arg1, int arg2) {
        native_onDescriptorWrite(ptr, arg0, arg1, arg2);
    }
    private native void native_onDescriptorWrite(long ptr, android.bluetooth.BluetoothGatt arg0, android.bluetooth.BluetoothGattDescriptor arg1, int arg2);

    @Override
    public void onReliableWriteCompleted(android.bluetooth.BluetoothGatt arg0, int arg1) {
        native_onReliableWriteCompleted(ptr, arg0, arg1);
    }
    private native void native_onReliableWriteCompleted(long ptr, android.bluetooth.BluetoothGatt arg0, int arg1);

    @Override
    public void onReadRemoteRssi(android.bluetooth.BluetoothGatt arg0, int arg1, int arg2) {
        native_onReadRemoteRssi(ptr, arg0, arg1, arg2);
    }
    private native void native_onReadRemoteRssi(long ptr, android.bluetooth.BluetoothGatt arg0, int arg1, int arg2);

    @Override
    public void onMtuChanged(android.bluetooth.BluetoothGatt arg0, int arg1, int arg2) {
        native_onMtuChanged(ptr, arg0, arg1, arg2);
    }
    private native void native_onMtuChanged(long ptr, android.bluetooth.BluetoothGatt arg0, int arg1, int arg2);

    @Override
    public void onServiceChanged(android.bluetooth.BluetoothGatt arg0) {
        native_onServiceChanged(ptr, arg0);
    }
    private native void native_onServiceChanged(long ptr, android.bluetooth.BluetoothGatt arg0);

}
