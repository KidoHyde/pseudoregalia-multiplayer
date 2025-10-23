#pragma once

#include <string>



#include "ST_PlayerInfo.hpp"

namespace Client
{
    void OnSceneLoad(std::wstring);
    void Tick();
    uint32_t SetPlayerInfo(const FST_PlayerInfo&);
    void GetGhostInfo(const uint32_t&, RC::Unreal::FScriptArray&, RC::Unreal::TArray<uint8_t>&);
}
