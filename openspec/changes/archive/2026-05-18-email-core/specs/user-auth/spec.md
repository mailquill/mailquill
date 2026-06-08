## ADDED Requirements

### Requirement: User registration
The system SHALL allow new users to register with an email address and password. Passwords MUST be hashed using Argon2id before storage.

#### Scenario: Successful registration
- **WHEN** a user submits a valid email and password (min 8 characters)
- **THEN** the system creates the account, returns a 201, and issues an access token + refresh token

#### Scenario: Duplicate email
- **WHEN** a user registers with an already-registered email
- **THEN** the system returns 409 Conflict

#### Scenario: Weak password
- **WHEN** a user submits a password shorter than 8 characters
- **THEN** the system returns 422 with a validation error

---

### Requirement: User login
The system SHALL authenticate users with email and password and issue JWT access tokens and httpOnly refresh tokens.

#### Scenario: Successful login
- **WHEN** a user submits valid credentials
- **THEN** the system returns a signed JWT (15-minute expiry) and sets a httpOnly refresh token cookie (30-day expiry)

#### Scenario: Invalid credentials
- **WHEN** a user submits wrong email or password
- **THEN** the system returns 401 with a generic error (no field-level distinction)

---

### Requirement: Token refresh
The system SHALL issue a new access token when a valid refresh token is presented.

#### Scenario: Valid refresh token
- **WHEN** a request is made to `/auth/refresh` with a valid httpOnly refresh token cookie
- **THEN** the system returns a new access token

#### Scenario: Expired or invalid refresh token
- **WHEN** the refresh token is expired, revoked, or tampered
- **THEN** the system returns 401 and clears the cookie

---

### Requirement: Logout
The system SHALL invalidate the refresh token on logout.

#### Scenario: Logout
- **WHEN** a user calls `/auth/logout`
- **THEN** the refresh token is revoked server-side and the cookie is cleared

---

### Requirement: Protected routes
All API endpoints except `/auth/register`, `/auth/login`, `/auth/refresh`, and OAuth callbacks SHALL require a valid JWT.

#### Scenario: Missing or invalid token
- **WHEN** a request is made without a valid Authorization header
- **THEN** the system returns 401
