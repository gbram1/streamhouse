package io.streamhouse.client.exception;

public class TimeoutException extends StreamHouseException {
    public TimeoutException(String message) {
        super(message, 408);
    }
}
