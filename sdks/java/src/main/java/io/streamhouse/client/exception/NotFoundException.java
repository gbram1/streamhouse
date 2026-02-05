package io.streamhouse.client.exception;

public class NotFoundException extends StreamHouseException {
    public NotFoundException(String message) {
        super(message, 404);
    }
}
