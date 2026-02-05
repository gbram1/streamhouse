package io.streamhouse.client.exception;

public class ConflictException extends StreamHouseException {
    public ConflictException(String message) {
        super(message, 409);
    }
}
