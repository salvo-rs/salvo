-- Your SQL goes here
CREATE TABLE "users"(
	"id" UUID NOT NULL PRIMARY KEY,
	"username" VARCHAR NOT NULL,
	"password" VARCHAR NOT NULL,
	"full_name" VARCHAR NOT NULL
);

CREATE TABLE "posts"(
	"id" UUID NOT NULL PRIMARY KEY,
	"title" VARCHAR NOT NULL,
	"content" TEXT NOT NULL,
	"user_id" UUID NOT NULL,
	FOREIGN KEY ("user_id") REFERENCES "users"("id")
);

