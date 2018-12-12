/*
 * Licensed to the Apache Software Foundation (ASF) under one
 * or more contributor license agreements.  See the NOTICE file
 * distributed with this work for additional information
 * regarding copyright ownership.  The ASF licenses this file
 * to you under the Apache License, Version 2.0 (the
 * "License"); you may not use this file except in compliance
 * with the License.  You may obtain a copy of the License at
 *
 *  http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */
#ifndef __LOGGING_H__
#define __LOGGING_H__

#include <stdio.h>

#define LOG_LEVEL_OFF     1
#define LOG_LEVEL_ERROR   2
#define LOG_LEVEL_WARNING 3
#define LOG_LEVEL_INFO    4
#define LOG_LEVEL_DEBUG   5

#ifndef LOG_LEVEL
#define LOG_LEVEL MYNEWT_VAL(LOG_LEVEL)
#endif

#if !((LOG_LEVEL >= LOG_LEVEL_OFF) && (LOG_LEVEL <= LOG_LEVEL_DEBUG))
#error "Invalid LOG_LEVEL config."
#endif

#if LOG_LEVEL >= LOG_LEVEL_ERROR
#define LOG_ERR(_fmt, ...)                                              \
    do {                                                                \
        printf("[ERR] " _fmt "\n", ##__VA_ARGS__);                      \
    } while (0)
#else
#define LOG_ERR(...)
#endif

#if LOG_LEVEL >= LOG_LEVEL_WARNING
#define LOG_WRN(_fmt, ...)                                              \
    do {                                                                \
        printf("[WRN] " _fmt "\n", ##__VA_ARGS__);                      \
    } while (0)
#else
#define MCUBOOT_LOG_WRN(...)
#endif

#if LOG_LEVEL >= LOG_LEVEL_INFO
#define LOG_INF(_fmt, ...)                                              \
    do {                                                                \
        printf("[INF] " _fmt "\n", ##__VA_ARGS__);                      \
    } while (0)
#else
#define LOG_INF(...)
#endif

#if LOG_LEVEL >= LOG_LEVEL_DEBUG
#define LOG_DBG(_fmt, ...)                                              \
    do {                                                                \
        printf("[DBG] " _fmt "\n", ##__VA_ARGS__);                      \
    } while (0)
#else
#define LOG_DBG(...)
#endif

#endif
