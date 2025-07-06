#pragma once

#include <valhalla/loki/worker.h>
#include <valhalla/odin/worker.h>
#include <valhalla/thor/worker.h>

#include "cxx.h"

/// Copy&paste of the `valhalla::tyr::actor_t` class, but without the parsing json request format.
struct Actor final {
  std::shared_ptr<valhalla::baldr::GraphReader> reader;
  valhalla::loki::loki_worker_t loki_worker;
  valhalla::thor::thor_worker_t thor_worker;
  valhalla::odin::odin_worker_t odin_worker;

  Actor() : reader{}, loki_worker({}, reader), thor_worker({}, reader), odin_worker({}) {}

  // Actor(const boost::property_tree::ptree& config)
  //     : reader(new valhalla::baldr::GraphReader(config.get_child("mjolnir"))),
  //       loki_worker(config, reader),
  //       thor_worker(config, reader),
  //       odin_worker(config) {}
  //

  rust::string trace_route(rust::slice<const uint8_t> request) {
    valhalla::Api api;
    api.ParseFromArray(request.data(), request.size());

    loki_worker.trace(api);
    thor_worker.trace_route(api);
    return odin_worker.narrate(api);
  }

  rust::string race_attributes(rust::slice<const uint8_t> request) {
    valhalla::Api api;
    api.ParseFromArray(request.data(), request.size());

    loki_worker.trace(api);
    return thor_worker.trace_attributes(api);
  }
};

std::unique_ptr<Actor> new_actor() {
  // todo: parse config and pass it to the Actor constructor
  return std::make_unique<Actor>();
}
