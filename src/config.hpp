#pragma once

#include <valhalla/baldr/rapidjson_utils.h>
#include <boost/property_tree/ptree.hpp>

#include "rust/cxx.h"

std::unique_ptr<boost::property_tree::ptree> from_file(rust::slice<const uint8_t> path) {
  auto pt = std::make_unique<boost::property_tree::ptree>();
  try {
    std::string str(reinterpret_cast<const char*>(path.data()), path.size());
    rapidjson::read_json(str, *pt);
  } catch (const std::exception& e) {
    throw std::runtime_error("Failed to read config file: " + std::string(e.what()));
  }
  return pt;
}

std::unique_ptr<boost::property_tree::ptree> from_json(rust::str config) {
  rapidjson::Document d;
  d.Parse(config.data(), config.size());
  if (d.HasParseError()) {
    throw std::runtime_error("Could not parse json, error at offset: " + std::to_string(d.GetErrorOffset()));
  }

  auto pt = std::make_unique<boost::property_tree::ptree>();
  if (d.IsObject()) {
    rapidjson::add_object(const_cast<const rapidjson::Document*>(&d)->GetObject(), *pt);
  } else if (d.IsArray()) {
    rapidjson::add_array(const_cast<const rapidjson::Document*>(&d)->GetArray(), *pt);
  } else {
    throw std::runtime_error("Json is not an object or array");
  }
  return pt;
}
