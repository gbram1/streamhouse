package io.streamhouse.client.exception;

public class AuthenticationException extends StreamHouseException {
    public AuthenticationException(String message) {
        super(message, 401);
    }
}
