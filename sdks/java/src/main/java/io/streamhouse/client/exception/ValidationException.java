package io.streamhouse.client.exception;

public class ValidationException extends StreamHouseException {
    public ValidationException(String message) {
        super(message, 400);
    }
}
