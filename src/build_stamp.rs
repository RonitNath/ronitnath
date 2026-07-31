pub const BUILD_STAMP: http_runtime::BuildStamp = http_runtime::BuildStamp::new(
    env!("WEB_BUILD_GIT_REVISION"),
    env!("WEB_BUILD_UNIX_TIME"),
    "ronitnath",
);
